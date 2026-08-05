//! AST -> lowered-IR lowering pass, split out of `eval.rs`.

use crate::modules::{ModuleFnSig, RuntimeOp, api_spec};
use crate::runtime::value::{DurationValue, PathValue, RecordMap, RuntimeError, Value};
use crate::sema::check::{CompactBodyProbeOutput, CompactDeclOutput, CompactTypeDefInfo};
use crate::sema::records::standard_record_type;
use crate::sema::types::{CallableParamType, ModuleExportType, Type};
use crate::source::{SourceMap, Span};
use crate::symbol::{Name, QualifiedName, Symbol};
use crate::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCallArg,
    ArenaCallArgKind, ArenaCommand, ArenaCommandArgKind, ArenaExprKind, ArenaExprOrRun,
    ArenaFmtPart, ArenaPatternKind, ArenaPipeStageKind, ArenaProgram, ArenaRecordFieldKind,
    ArenaSpawnTarget, ArenaStmtKind, ArenaStreamStage, ArenaTypeExprTag, ArenaWordPart, AstArena,
    BindingTargetId, BlockId, ExprId, FunctionDefId, PatternId, StmtId, TypeExprId,
};
use crate::syntax::node::{
    AssignOp, BinaryOp, CommandWordRefSegment, CoreCommand, EnvGetKind, RunKind, StreamStageKind,
    UnaryOp, parse_command_word_reference,
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
    LoweredRunCapture, LoweredSpawnRun, lowered_record_vec_get, lowered_record_vec_get_mut,
    lowered_record_vec_insert,
};

/// Name -> dense slot-index map used while lowering a function or top-level
/// region. Slots are allocated densely and never reused; `high_water` is the
/// runtime slot-array size, so retired names do not need synthetic keys.
#[derive(Clone, Default)]
pub(super) struct SlotScope {
    indices: FxHashMap<Name, usize>,
    types: FxHashMap<Name, Type>,
    captures: FxHashSet<Name>,
    // (name, previous slot) for each block-local declaration, so `exit` can
    // restore a shadowed outer binding (or drop a freshly-introduced one).
    declared: Vec<(Name, Option<usize>, Option<Type>, bool)>,
    high_water: usize,
}

/// Snapshot of a `SlotScope` taken on entering a nested block (see `enter`/`exit`).
pub(super) struct SlotSnapshot {
    declared_len: usize,
    high_water: usize,
}

struct LoweredFsFilesArgs {
    root: ExprId,
    gitignore: bool,
    stat: bool,
    hidden: bool,
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

struct LoweredFsWriteArgs {
    path: ExprId,
    data: ExprId,
}

struct LoweredFsMkdirArgs {
    path: ExprId,
    parents: Option<ExprId>,
}

struct LoweredFsRemoveArgs {
    path: ExprId,
    missing_ok: Option<ExprId>,
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

struct LoweredModuleCallArgs {
    op: RuntimeOp,
    args: Vec<ExprId>,
}

struct LoweredHashVerifyFileArgs {
    path: ExprId,
    algorithm: crate::modules::hash::HashAlgorithm,
    expected: ExprId,
}

struct LoweredAbortArgs {
    status: ExprId,
    force: Option<ExprId>,
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
        "byte_at" => args.len() == 1 || args.len() == 2,
        _ => false,
    }
}

fn lowered_method_call_args(name: Name, args: &[ArenaCallArg]) -> Option<Vec<ExprId>> {
    if let Some(positional) = positional_call_args(args) {
        return Some(positional);
    }
    match name.as_str().as_str() {
        "format" => named_method_call_args(args, &[], &["precision"]),
        "squeeze" => named_method_call_args(args, &[], &["chars"]),
        "fields" => named_method_call_args(args, &[], &["delimiter"]),
        "join" => named_method_call_args(args, &[], &["separator"]),
        "split" => named_method_call_args(args, &["separator"], &["maxsplit"]),
        "dump" => named_method_call_args(args, &[], &["format"]),
        "strings" => named_method_call_args(args, &[], &["min_len"]),
        "wrap" => named_method_call_args(args, &["width"], &[]),
        "chunks" => named_method_call_args(args, &["size"], &[]),
        "byte_at" => named_method_call_args(args, &["index"], &["default"]),
        "byte_slice" | "slice" => named_method_call_args(args, &["offset"], &["length"]),
        "cancel" => named_method_call_args(args, &[], &["signal", "kill_after"]),
        "chmod" => named_method_call_args(args, &["mode"], &[]),
        "copy" | "rename" => named_method_call_args(args, &["dest"], &["overwrite"]),
        "find" => named_method_call_args(args, &["needle"], &["start"]),
        "hardlink" => named_method_call_args(args, &["path"], &[]),
        "touch" => named_method_call_args(args, &[], &["create"]),
        "touch_from" => named_method_call_args(args, &["reference"], &[]),
        "truncate" => named_method_call_args(args, &["size"], &[]),
        _ => None,
    }
}

fn named_method_call_args(
    args: &[ArenaCallArg],
    required: &[&str],
    optional: &[&str],
) -> Option<Vec<ExprId>> {
    let mut names = Vec::with_capacity(required.len() + optional.len());
    names.extend_from_slice(required);
    names.extend_from_slice(optional);
    let mut bindings = vec![None; names.len()];
    let mut next_positional = 0usize;
    for arg in args {
        match arg.kind {
            ArenaCallArgKind::Splice { .. } => return None,
            ArenaCallArgKind::Positional(value) => {
                while next_positional < bindings.len() && bindings[next_positional].is_some() {
                    next_positional += 1;
                }
                let binding = bindings.get_mut(next_positional)?;
                *binding = Some(value);
            }
            ArenaCallArgKind::Named { name, value, .. } => {
                let index = names
                    .iter()
                    .position(|candidate| *candidate == name.as_str())?;
                if bindings[index].replace(value).is_some() {
                    return None;
                }
            }
        }
    }
    if bindings.iter().take(required.len()).any(Option::is_none) {
        return None;
    }
    // Compact the bound arguments in parameter order, dropping skipped optionals.
    // Required args cannot be skipped (checked above); only methods with 2+
    // optionals (currently just `cancel`) can leave a gap, and their runtime
    // handlers dispatch the trailing optionals by type rather than position.
    Some(bindings.into_iter().flatten().collect())
}

fn lower_abort_args(args: &[ArenaCallArg]) -> Option<LoweredAbortArgs> {
    let first = args.first()?;
    let status = match &first.kind {
        ArenaCallArgKind::Positional(value) => *value,
        ArenaCallArgKind::Named { name, value, .. } if *name == "status" => *value,
        ArenaCallArgKind::Named { .. } | ArenaCallArgKind::Splice { .. } => return None,
    };
    let force = match args.get(1).map(|arg| &arg.kind) {
        None => None,
        Some(ArenaCallArgKind::Positional(value)) => Some(*value),
        Some(ArenaCallArgKind::Named { name, value, .. }) if *name == "force" => Some(*value),
        Some(ArenaCallArgKind::Named { .. } | ArenaCallArgKind::Splice { .. }) => return None,
    };
    if args.len() > 2 {
        return None;
    }
    Some(LoweredAbortArgs { status, force })
}

fn lower_process_command_argv_args(args: &[ArenaCallArg]) -> Option<LoweredProcessCommandArgvArgs> {
    let names = [
        "target",
        "argv",
        "cwd",
        "env",
        "stdin",
        "stdout",
        "stderr",
        "stdout_append",
        "stderr_append",
        "timeout",
        "detach",
        "new_session",
        "ignore_hup",
        "cpu_max",
    ];
    if !(2..=names.len()).contains(&args.len()) {
        return None;
    }
    let mut slots: [Option<ExprId>; 14] = [None; 14];
    let mut next_positional = 0usize;
    for arg in args {
        match arg.kind {
            ArenaCallArgKind::Named { name, value, .. } => {
                let index = names.iter().position(|expected| name == *expected)?;
                if slots[index].is_some() {
                    return None;
                }
                slots[index] = Some(value);
            }
            ArenaCallArgKind::Positional(value) => {
                while next_positional < slots.len() && slots[next_positional].is_some() {
                    next_positional += 1;
                }
                let slot = slots.get_mut(next_positional)?;
                *slot = Some(value);
                next_positional += 1;
            }
            ArenaCallArgKind::Splice { .. } => return None,
        }
    }
    Some(LoweredProcessCommandArgvArgs {
        target: slots[0]?,
        argv: slots[1]?,
        cwd: slots[2],
        env: slots[3],
        stdin: slots[4],
        stdout: slots[5],
        stderr: slots[6],
        stdout_append: slots[7],
        stderr_append: slots[8],
        timeout: slots[9],
        detach: slots[10],
        new_session: slots[11],
        ignore_hup: slots[12],
        cpu_max: slots[13],
    })
}

fn lowered_module_call_args(
    module: Name,
    name: Name,
    args: &[ArenaCallArg],
) -> Option<LoweredModuleCallArgs> {
    if module == "fs" && name == "hardlink" {
        let positional = positional_call_args(args)?;
        if positional.len() == 2 {
            return Some(LoweredModuleCallArgs {
                op: RuntimeOp::FsHardlink,
                args: positional,
            });
        }
    }
    if module == "mime" && name == "lookup_ext" {
        let positional = positional_call_args(args)?;
        if positional.len() == 1 {
            return Some(LoweredModuleCallArgs {
                op: RuntimeOp::MimeLookupExt,
                args: positional,
            });
        }
    }
    if module == "mime" && name == "lookup_path" {
        let positional = positional_call_args(args)?;
        if positional.len() == 1 {
            return Some(LoweredModuleCallArgs {
                op: RuntimeOp::MimeLookupPath,
                args: positional,
            });
        }
    }
    let overloads = api_spec().module_overloads(&module.as_str(), &name.as_str())?;
    let mut matched = None;
    for sig in overloads {
        if lowered_module_sig_type(sig).is_none() {
            continue;
        }
        if let Some(bindings) = compact_module_bindings(args, sig) {
            matched = Some((sig, bindings));
            break;
        }
    }
    let (sig, bindings) = matched?;
    let mut lowered_args = Vec::new();
    for arg_index in bindings.into_iter().flatten() {
        lowered_args.push(compact_call_arg_expr(&args[arg_index])?);
    }
    Some(LoweredModuleCallArgs {
        op: sig.op,
        args: lowered_args,
    })
}

fn lower_hash_verify_file_args(args: &[ArenaCallArg]) -> Option<LoweredHashVerifyFileArgs> {
    let [path, checksum] = args else {
        return None;
    };
    let path = match path.kind {
        ArenaCallArgKind::Positional(expr) => expr,
        ArenaCallArgKind::Named { name, value, .. } if name == "path" => value,
        ArenaCallArgKind::Named { .. } | ArenaCallArgKind::Splice { .. } => return None,
    };
    let ArenaCallArgKind::Named { name, value, .. } = checksum.kind else {
        return None;
    };
    let algorithm = match name.as_str().as_str() {
        "md5" => crate::modules::hash::HashAlgorithm::Md5,
        "sha1" => crate::modules::hash::HashAlgorithm::Sha1,
        "sha256" => crate::modules::hash::HashAlgorithm::Sha256,
        "sha512" => crate::modules::hash::HashAlgorithm::Sha512,
        _ => return None,
    };
    Some(LoweredHashVerifyFileArgs {
        path,
        algorithm,
        expected: value,
    })
}

fn lowered_module_callee_type(module: Name, name: Name) -> Option<LoweredType> {
    api_spec()
        .module_overloads(&module.as_str(), &name.as_str())?
        .iter()
        .find_map(lowered_module_sig_type)
}

fn lowered_module_callee_result_ok_type(module: Name, name: Name) -> Option<LoweredType> {
    api_spec()
        .module_overloads(&module.as_str(), &name.as_str())?
        .iter()
        .filter(|sig| lowered_module_op_supported(sig.op))
        .find_map(|sig| sig.return_ty.result_ok().and_then(lowered_checked_type))
}

fn compact_module_bindings(args: &[ArenaCallArg], sig: &ModuleFnSig) -> Option<Vec<Option<usize>>> {
    let mut bindings = vec![None; sig.params.len()];
    let mut next_positional = 0usize;
    for (arg_index, arg) in args.iter().enumerate() {
        match arg.kind {
            ArenaCallArgKind::Splice { .. } => return None,
            ArenaCallArgKind::Positional(_) => {
                while next_positional < bindings.len() && bindings[next_positional].is_some() {
                    next_positional += 1;
                }
                let binding = bindings.get_mut(next_positional)?;
                *binding = Some(arg_index);
            }
            ArenaCallArgKind::Named { name, .. } => {
                let param_index = sig
                    .params
                    .iter()
                    .position(|param| param.name == name.as_str())?;
                if bindings[param_index].is_some() {
                    return None;
                }
                bindings[param_index] = Some(arg_index);
            }
        }
    }
    if sig
        .params
        .iter()
        .zip(&bindings)
        .any(|(param, binding)| !param.defaulted && binding.is_none())
    {
        return None;
    }
    Some(bindings)
}

fn compact_call_arg_expr(arg: &ArenaCallArg) -> Option<ExprId> {
    match arg.kind {
        ArenaCallArgKind::Positional(expr) | ArenaCallArgKind::Named { value: expr, .. } => {
            Some(expr)
        }
        ArenaCallArgKind::Splice { .. } => None,
    }
}

fn compact_named_function_call_arg_exprs(
    args: &[ArenaCallArg],
    params: &[CallableParamType],
) -> Option<Vec<ExprId>> {
    if !args
        .iter()
        .any(|arg| matches!(arg.kind, ArenaCallArgKind::Named { .. }))
    {
        return None;
    }
    let mut bindings = vec![None; params.len()];
    let mut next_positional = 0usize;
    for arg in args {
        match arg.kind {
            ArenaCallArgKind::Splice { .. } => return None,
            ArenaCallArgKind::Positional(value) => {
                while next_positional < bindings.len() && bindings[next_positional].is_some() {
                    next_positional += 1;
                }
                let param = params.get(next_positional)?;
                if param.rest {
                    return None;
                }
                bindings[next_positional] = Some(value);
                next_positional += 1;
            }
            ArenaCallArgKind::Named { name, value, .. } => {
                let index = params
                    .iter()
                    .position(|param| !param.rest && param.name == name)?;
                if bindings[index].replace(value).is_some() {
                    return None;
                }
            }
        }
    }
    for (param, binding) in params.iter().zip(&bindings) {
        if !param.defaulted && !param.rest && binding.is_none() {
            return None;
        }
    }
    let last = bindings.iter().rposition(Option::is_some)?;
    if bindings.iter().take(last).any(Option::is_none) {
        return None;
    }
    Some(
        bindings
            .into_iter()
            .take(last + 1)
            .map(|binding| binding.expect("checked dense call argument binding"))
            .collect(),
    )
}

fn lowered_module_sig_type(sig: &ModuleFnSig) -> Option<LoweredType> {
    if matches!(
        sig.op,
        RuntimeOp::TimeSleep | RuntimeOp::TimeFormat | RuntimeOp::TimeMeasure
    ) {
        return lowered_module_op_supported(sig.op).then_some(LoweredType::Result);
    }
    lowered_module_op_supported(sig.op).then(|| lowered_checked_type(&sig.return_ty))?
}

fn lowered_module_op_supported(op: RuntimeOp) -> bool {
    #[cfg(feature = "native-tests")]
    if lowered_native_test_op_supported(op) {
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
            | RuntimeOp::BytesCopy
            | RuntimeOp::BytesCopyFile
            | RuntimeOp::BytesFromInts
            | RuntimeOp::BytesConcat
            | RuntimeOp::BytesHuman
            | RuntimeOp::BytesPackLe
            | RuntimeOp::BytesPackBe
            | RuntimeOp::BytesUnpackLe
            | RuntimeOp::BytesUnpackBe
            | RuntimeOp::BytesReadAt
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
            | RuntimeOp::EnvBool
            | RuntimeOp::EnvPath
            | RuntimeOp::EnvInt
            | RuntimeOp::EnvList
            | RuntimeOp::EnvPathList
            | RuntimeOp::EnvPathEntries
            | RuntimeOp::FsCwd
            | RuntimeOp::FsDirs
            | RuntimeOp::FsLs
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
            | RuntimeOp::FsRootRead
            | RuntimeOp::FsRootReadText
            | RuntimeOp::FsRootWrite
            | RuntimeOp::FsRootWriteAtomic
            | RuntimeOp::FsRootMetadata
            | RuntimeOp::FsRootExists
            | RuntimeOp::FsRootMkdir
            | RuntimeOp::FsRootRemove
            | RuntimeOp::FsRootReadlink
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
            | RuntimeOp::FsFsync
            | RuntimeOp::FsSync
            | RuntimeOp::FsSymlink
            | RuntimeOp::FsLock
            | RuntimeOp::FsUnlock
            | RuntimeOp::FsTempFile
            | RuntimeOp::FsProjectRoot
            | RuntimeOp::FsUserRoot
            | RuntimeOp::FsGitroot
            | RuntimeOp::GroupCurrent
            | RuntimeOp::GroupLookup
            | RuntimeOp::GroupByGid
            | RuntimeOp::GroupAdd
            | RuntimeOp::GroupRemove
            | RuntimeOp::HashMd5
            | RuntimeOp::HashSha1
            | RuntimeOp::HashSha256
            | RuntimeOp::HashSha512
            | RuntimeOp::HashCrc32
            | RuntimeOp::HashCrc32c
            | RuntimeOp::HashParseCheckLine
            | RuntimeOp::HashVerifyFile
            | RuntimeOp::IoStdinBytes
            | RuntimeOp::IoStdinText
            | RuntimeOp::IoStdinLine
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
            | RuntimeOp::MimeLookupExt
            | RuntimeOp::MimeLookupPath
            | RuntimeOp::MimeParse
            | RuntimeOp::ModuleLoad
            | RuntimeOp::NetPool
            | RuntimeOp::NetClosePool
            | RuntimeOp::NetCloseAllPools
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
            | RuntimeOp::ProcessStats
            | RuntimeOp::ProcessWhich
            | RuntimeOp::ProcessPort
            | RuntimeOp::ProcessPorts
            | RuntimeOp::ProcessPortsForPid
            | RuntimeOp::ProcessSignal
            | RuntimeOp::ProcessKill
            | RuntimeOp::ProcessArgvWords
            | RuntimeOp::ProcessRun
            | RuntimeOp::ProcessSpawn
            | RuntimeOp::ProcessWaitAny
            | RuntimeOp::ProcessWaitReady
            | RuntimeOp::RecordRequire
            | RuntimeOp::RegexCompile
            | RuntimeOp::SetEmpty
            | RuntimeOp::SetFrom
            | RuntimeOp::SetHas
            | RuntimeOp::SetAdd
            | RuntimeOp::SetRemove
            | RuntimeOp::ShlexQuote
            | RuntimeOp::ShlexJoin
            | RuntimeOp::SystemHostname
            | RuntimeOp::SystemUname
            | RuntimeOp::SystemMemory
            | RuntimeOp::SystemOsRelease
            | RuntimeOp::TimeNow
            | RuntimeOp::TimeSleep
            | RuntimeOp::TimeMillis
            | RuntimeOp::TimeSeconds
            | RuntimeOp::TimeFormat
            | RuntimeOp::TimeMeasure
            | RuntimeOp::TimeDurationCompact
            | RuntimeOp::TuiReset
            | RuntimeOp::TuiBold
            | RuntimeOp::TuiDim
            | RuntimeOp::TuiRed
            | RuntimeOp::TuiGreen
            | RuntimeOp::TuiYellow
            | RuntimeOp::TuiBlue
            | RuntimeOp::TuiMagenta
            | RuntimeOp::TuiCyan
            | RuntimeOp::TuiWhite
            | RuntimeOp::TuiGray
            | RuntimeOp::TuiClear
            | RuntimeOp::TuiHome
            | RuntimeOp::TuiEraseLine
            | RuntimeOp::TuiHideCursor
            | RuntimeOp::TuiShowCursor
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
            | RuntimeOp::UserLookup
            | RuntimeOp::UserByUid
            | RuntimeOp::UserAdd
            | RuntimeOp::UserRemove
            | RuntimeOp::UtilsCache
    )
}

#[cfg(feature = "native-tests")]
fn lowered_native_test_op_supported(op: RuntimeOp) -> bool {
    matches!(
        op,
        RuntimeOp::TestOk
            | RuntimeOp::TestEq
            | RuntimeOp::TestNe
            | RuntimeOp::TestContains
            | RuntimeOp::TestNotContains
            | RuntimeOp::TestErrorKind
            | RuntimeOp::TestFail
            | RuntimeOp::TestSkip
            | RuntimeOp::TestTempPath
            | RuntimeOp::TestTempDir
            | RuntimeOp::TestTempFile
            | RuntimeOp::TestMock
            | RuntimeOp::TestCalls
            | RuntimeOp::TestRunScript
            | RuntimeOp::TestRunXsh
            | RuntimeOp::TestRunXshtTrace
    )
}

fn lower_archive_tar_create_args(args: &[ArenaCallArg]) -> Option<LoweredArchiveTarCreateArgs> {
    let mut positional = Vec::with_capacity(3);
    let mut compression = None;
    let mut overwrite = None;
    for arg in args {
        match arg.kind {
            ArenaCallArgKind::Positional(value) => match positional.len() {
                0..=2 => positional.push(value),
                3 => {
                    if compression.replace(value).is_some() {
                        return None;
                    }
                    positional.push(value);
                }
                4 => {
                    if overwrite.replace(value).is_some() {
                        return None;
                    }
                    positional.push(value);
                }
                _ => return None,
            },
            ArenaCallArgKind::Named { name, value, .. } if name == "compression" => {
                if positional.len() > 3 || compression.replace(value).is_some() {
                    return None;
                }
            }
            ArenaCallArgKind::Named { name, value, .. } if name == "overwrite" => {
                if positional.len() > 4 {
                    return None;
                }
                if overwrite.replace(value).is_some() {
                    return None;
                }
            }
            ArenaCallArgKind::Named { .. } | ArenaCallArgKind::Splice { .. } => return None,
        }
    }
    if positional.len() < 3 {
        return None;
    }
    Some(LoweredArchiveTarCreateArgs {
        path: positional[0],
        root: positional[1],
        entries: positional[2],
        compression,
        overwrite,
    })
}

fn lower_fs_write_args(args: &[ArenaCallArg]) -> Option<LoweredFsWriteArgs> {
    let mut path = None;
    let mut data = None;
    for arg in args {
        match arg.kind {
            ArenaCallArgKind::Positional(value) if path.is_none() => path = Some(value),
            ArenaCallArgKind::Positional(value) if data.is_none() => data = Some(value),
            ArenaCallArgKind::Positional(_) => return None,
            ArenaCallArgKind::Named { name, value, .. } if name == "path" => {
                if path.replace(value).is_some() {
                    return None;
                }
            }
            ArenaCallArgKind::Named { name, value, .. } if name == "data" => {
                if data.replace(value).is_some() {
                    return None;
                }
            }
            ArenaCallArgKind::Named { .. } | ArenaCallArgKind::Splice { .. } => return None,
        }
    }
    Some(LoweredFsWriteArgs {
        path: path?,
        data: data?,
    })
}

fn lower_fs_mkdir_args(args: &[ArenaCallArg]) -> Option<LoweredFsMkdirArgs> {
    let mut path = None;
    let mut parents = None;
    let mut next_positional = 0usize;
    for arg in args {
        match arg.kind {
            ArenaCallArgKind::Positional(value) => {
                match next_positional {
                    0 => {
                        if path.is_some() {
                            return None;
                        }
                        path = Some(value);
                    }
                    1 => {
                        if parents.is_some() {
                            return None;
                        }
                        parents = Some(value);
                    }
                    _ => return None,
                }
                next_positional += 1;
            }
            ArenaCallArgKind::Named { name, value, .. } if name == "path" => {
                if path.replace(value).is_some() {
                    return None;
                }
            }
            ArenaCallArgKind::Named { name, value, .. } if name == "parents" => {
                if parents.replace(value).is_some() {
                    return None;
                }
            }
            ArenaCallArgKind::Named { .. } | ArenaCallArgKind::Splice { .. } => return None,
        }
    }
    Some(LoweredFsMkdirArgs {
        path: path?,
        parents,
    })
}

fn lower_fs_remove_args(args: &[ArenaCallArg]) -> Option<LoweredFsRemoveArgs> {
    let mut path = None;
    let mut missing_ok = None;
    let mut next_positional = 0usize;
    for arg in args {
        match arg.kind {
            ArenaCallArgKind::Positional(value) => {
                match next_positional {
                    0 => {
                        if path.is_some() {
                            return None;
                        }
                        path = Some(value);
                    }
                    1 => {
                        if missing_ok.is_some() {
                            return None;
                        }
                        missing_ok = Some(value);
                    }
                    _ => return None,
                }
                next_positional += 1;
            }
            ArenaCallArgKind::Named { name, value, .. } if name == "path" => {
                if path.replace(value).is_some() {
                    return None;
                }
            }
            ArenaCallArgKind::Named { name, value, .. } if name == "missing_ok" => {
                if missing_ok.replace(value).is_some() {
                    return None;
                }
            }
            ArenaCallArgKind::Named { .. } | ArenaCallArgKind::Splice { .. } => return None,
        }
    }
    Some(LoweredFsRemoveArgs {
        path: path?,
        missing_ok,
    })
}

fn lower_path_mkdir_args(args: &[ArenaCallArg]) -> Option<LoweredPathMkdirArgs> {
    let mut parents = None;
    for arg in args {
        match arg.kind {
            ArenaCallArgKind::Named { name, value, .. } if name == "parents" => {
                if parents.replace(value).is_some() {
                    return None;
                }
            }
            ArenaCallArgKind::Positional(_)
            | ArenaCallArgKind::Named { .. }
            | ArenaCallArgKind::Splice { .. } => {
                return None;
            }
        }
    }
    Some(LoweredPathMkdirArgs { parents })
}

fn lower_path_remove_args(args: &[ArenaCallArg]) -> Option<LoweredPathRemoveArgs> {
    let mut missing_ok = None;
    for arg in args {
        match arg.kind {
            ArenaCallArgKind::Named { name, value, .. } if name == "missing_ok" => {
                if missing_ok.replace(value).is_some() {
                    return None;
                }
            }
            ArenaCallArgKind::Positional(_)
            | ArenaCallArgKind::Named { .. }
            | ArenaCallArgKind::Splice { .. } => {
                return None;
            }
        }
    }
    Some(LoweredPathRemoveArgs { missing_ok })
}

fn lower_path_write_args(args: &[ArenaCallArg]) -> Option<LoweredPathWriteArgs> {
    let positional = positional_call_args(args)?;
    let [data] = positional.as_slice() else {
        return None;
    };
    Some(LoweredPathWriteArgs { data: *data })
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
) -> Option<BuildExprId> {
    let (root, segments) = parse_command_word_reference(text)?;
    let mut value = if let Some(slot) = slots.resolve(Name::intern(root)) {
        build_expr(scratch, BuildExprRow::Param(slot))
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
                        op: RuntimeOp::EnvPathList,
                        args: vec![name],
                        span,
                    },
                ))
            } else {
                let name = build_expr(scratch, BuildExprRow::Str(name.to_string().into()));
                Some(build_expr(
                    scratch,
                    BuildExprRow::ModuleCall {
                        op: RuntimeOp::EnvGet,
                        args: vec![name],
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
                    op,
                    args: vec![name],
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
        RunKind::StreamText | RunKind::StreamBytes => Some(LoweredType::List),
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
    let [segment] = arena.run_segments(run.segments) else {
        return None;
    };
    lowered_run_capture_type(segment.kind)
}

fn lowered_arena_run_binding_type(
    arena: &AstArena,
    id: crate::syntax::arena::RunFormId,
) -> Option<LoweredType> {
    let run = arena.run_form(id);
    match arena.run_segments(run.segments) {
        [] => None,
        [segment] => lowered_run_binding_type(segment.kind),
        // A multi-segment pipeline always evaluates to the pipeline Status
        // (eval_lowered_run_pipeline returns LoweredValue::Status), so the
        // binding takes Status regardless of the individual segment kinds.
        _ => Some(LoweredType::Status),
    }
}

fn lowered_arena_run_status_type(
    arena: &AstArena,
    id: crate::syntax::arena::RunFormId,
) -> Option<LoweredType> {
    let run = arena.run_form(id);
    let [segment] = arena.run_segments(run.segments) else {
        return None;
    };
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

fn lower_fs_files_args(
    arena: &AstArena,
    args: &[ArenaCallArg],
    has_exts: bool,
) -> Option<LoweredFsFilesArgs> {
    let mut root = None;
    let mut gitignore = true;
    let mut stat = true;
    let mut hidden = false;
    let mut exts = None;
    let mut next_positional = 0usize;
    for arg in args {
        match arg.kind {
            ArenaCallArgKind::Positional(value) => {
                // Optional arguments may be passed positionally in parameter
                // order (path, gitignore, stat, exts, hidden). `walk`/`dirs`
                // share this helper but have no `exts` parameter, so a fourth
                // positional maps directly to `hidden` for them.
                match next_positional {
                    0 => {
                        if root.is_some() {
                            return None;
                        }
                        root = Some(value);
                    }
                    1 => gitignore = arena_bool_literal(arena, value)?,
                    2 => stat = arena_bool_literal(arena, value)?,
                    3 if has_exts => exts = Some(value),
                    3 => hidden = arena_bool_literal(arena, value)?,
                    4 => hidden = arena_bool_literal(arena, value)?,
                    _ => return None,
                }
                next_positional += 1;
            }
            ArenaCallArgKind::Named { name, value, .. } if name == "gitignore" => {
                gitignore = arena_bool_literal(arena, value)?;
            }
            ArenaCallArgKind::Named { name, value, .. } if name == "stat" => {
                stat = arena_bool_literal(arena, value)?;
            }
            ArenaCallArgKind::Named { name, value, .. } if name == "hidden" => {
                hidden = arena_bool_literal(arena, value)?;
            }
            ArenaCallArgKind::Named { name, value, .. } if name == "exts" => {
                exts = Some(value);
            }
            ArenaCallArgKind::Named { .. } | ArenaCallArgKind::Splice { .. } => return None,
        }
    }
    Some(LoweredFsFilesArgs {
        root: root?,
        gitignore,
        stat,
        hidden,
        exts,
    })
}

fn lower_fs_list_args(args: &[ArenaCallArg]) -> Option<LoweredFsListArgs> {
    let mut path = None;
    let mut stat = None;
    let mut ordered = None;
    let mut next_positional = 0usize;
    for arg in args {
        match arg.kind {
            ArenaCallArgKind::Positional(value) => {
                let target = match next_positional {
                    0 => &mut path,
                    1 => &mut stat,
                    2 => &mut ordered,
                    _ => return None,
                };
                if target.is_some() {
                    return None;
                }
                *target = Some(value);
                next_positional += 1;
            }
            ArenaCallArgKind::Named { name, value, .. } if name == "path" => {
                if path.is_some() {
                    return None;
                }
                path = Some(value);
            }
            ArenaCallArgKind::Named { name, value, .. } if name == "stat" => {
                if stat.is_some() {
                    return None;
                }
                stat = Some(value);
            }
            ArenaCallArgKind::Named { name, value, .. } if name == "ordered" => {
                if ordered.is_some() {
                    return None;
                }
                ordered = Some(value);
            }
            ArenaCallArgKind::Named { .. } | ArenaCallArgKind::Splice { .. } => return None,
        }
    }
    Some(LoweredFsListArgs {
        path: path?,
        stat,
        ordered,
    })
}

fn arena_bool_literal(arena: &AstArena, expr: ExprId) -> Option<bool> {
    match arena.expr(expr).kind {
        ArenaExprKind::Bool(value) => Some(value),
        // Non-literal expressions (variables, calls, etc.) are treated as true
        // — the runtime will evaluate the actual bool value.
        _ => Some(true),
    }
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
            types: FxHashMap::default(),
            captures: FxHashSet::default(),
            declared: Vec::new(),
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
        let slot = self.declare(name);
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
    pub(super) fn retire(&mut self, name: Name, _slot: usize, _tag: &str) {
        self.indices.remove(&name);
    }

    /// Snapshot bindings on entering a nested block scope.
    pub(super) fn enter(&self) -> SlotSnapshot {
        SlotSnapshot {
            declared_len: self.declared.len(),
            high_water: self.high_water,
        }
    }

    /// Restore name resolution to the block-entry snapshot, dropping block-local
    /// bindings while keeping every slot index allocated inside the block.
    /// A block-local declaration that shadowed an outer binding restores the
    /// outer slot; a freshly-introduced one is dropped.
    pub(super) fn exit(&mut self, snapshot: SlotSnapshot) {
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
    LowerableFunctions, LoweredCallArg, LoweredCompFields, LoweredCompTarget, LoweredErrorExpr,
    LoweredErrorPatternFields, LoweredFmtPart, LoweredFunctionBlocker, LoweredFunctionKey,
    LoweredFunctionKind, LoweredFunctionUnit, LoweredModuleExport, LoweredModuleExportKind,
    LoweredParamChecks, LoweredParamDefaults, LoweredParamKinds, LoweredParamNames,
    LoweredParamRest, LoweredPipelineStage, LoweredReturnKind, LoweredRunArg, LoweredRunArgKind,
    LoweredRunEnv, LoweredRunPipelineSegment, LoweredRunRedirection, LoweredStrPredicate,
    LoweredTopLevelBinding, LoweredTopLevelSlot, LoweredTopLevelSlots, LoweredType,
    LoweredTypeCheck, LoweredValue, ProgramBuild, ReduceByOp, ScanBytes, ScanCheck, ScanCondition,
    StmtFlow, lowered_method_name,
};

pub(super) fn lowered_arena_type(
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
) -> Option<LoweredType> {
    lowered_arena_type_inner(arena, ty, declarations, 0)
}

fn lowered_arena_result_ok_type(
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
) -> Option<LoweredType> {
    if arena.type_expr_tags[ty.index()] != ArenaTypeExprTag::Result {
        return None;
    }
    let data = arena.type_expr_data[ty.index()];
    lowered_arena_type(
        arena,
        TypeExprId::from_index(data.lhs as usize),
        declarations,
    )
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
                Some(CompactTypeDefInfo::Alias(alias)) => {
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
                Some(CompactTypeDefInfo::Alias(alias)) => {
                    lowered_arena_type_inner(arena, *alias, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Record(_)) => Some(LoweredType::Record),
                Some(CompactTypeDefInfo::Module(_)) => Some(LoweredType::Module),
                Some(CompactTypeDefInfo::TagUnion) => Some(LoweredType::Tag),
                None => Some(LoweredType::Record),
            }
        }
        ArenaTypeExprTag::List => Some(LoweredType::List),
        ArenaTypeExprTag::Map => Some(LoweredType::Map),
        ArenaTypeExprTag::Stream => Some(LoweredType::Stream),
        ArenaTypeExprTag::Module => Some(LoweredType::Module),
        ArenaTypeExprTag::Result => Some(LoweredType::Result),
        ArenaTypeExprTag::Optional => Some(LoweredType::Any),
    }
}

pub(super) fn probe_compact_lower_constructed_bodies(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    bodies: &CompactBodyProbeOutput,
    source: &str,
) -> CompactLowerConstructProbeOutput {
    let mut probe = CompactLowerConstructProbe {
        program,
        declarations,
        bodies,
        source,
        sources: None,
        current_namespace: None,
        functions: None,
        top_level_known: FxHashMap::default(),
        output: CompactLowerConstructProbeOutput {
            expr_type_facts: bodies.expr_types.len(),
            ..CompactLowerConstructProbeOutput::default()
        },
        last_blocker_detail: None,
        strict_dynamic_methods: true,
        scratch: Rc::new(RefCell::new(BuildScratch::default())),
    };
    probe.probe_program();
    probe.output
}

pub(super) fn lower_compact_function_units_into(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    bodies: &CompactBodyProbeOutput,
    source: &str,
    sources: &SourceMap,
    mut emit: impl FnMut(LoweredFunctionUnit) -> Result<(), super::indexed::IrBuildError>,
) -> Result<(), super::indexed::IrBuildError> {
    let root = compact_function_defs(program);
    let candidates = root.iter().map(|function| function.key).collect::<Vec<_>>();
    let index_of = root
        .iter()
        .enumerate()
        .map(|(index, function)| (function.key, index))
        .collect::<FxHashMap<_, _>>();
    let adjacency = root
        .iter()
        .map(|function| {
            compact_function_call_edges(program, function.id, function.namespace, &index_of)
        })
        .collect::<Vec<_>>();
    let dependencies = adjacency
        .iter()
        .map(|edges| {
            edges
                .iter()
                .map(|&index| root[index].key)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut scc_metadata = vec![(1, None); root.len()];
    for pure in [true, false] {
        let functions = root
            .iter()
            .enumerate()
            .filter(|(_, function)| function.pure == pure)
            .collect::<Vec<_>>();
        let kind_index_of = functions
            .iter()
            .enumerate()
            .map(|(index, (_, function))| (function.key, index))
            .collect::<FxHashMap<_, _>>();
        let kind_adjacency = functions
            .iter()
            .map(|(_, function)| {
                compact_function_call_edges(
                    program,
                    function.id,
                    function.namespace,
                    &kind_index_of,
                )
            })
            .collect::<Vec<_>>();
        for (group, scc) in compact_tarjan_sccs(kind_adjacency).into_iter().enumerate() {
            let member_count = scc.len();
            let group = (member_count > 1).then_some(group);
            for index in scc {
                scc_metadata[functions[index].0] = (member_count, group);
            }
        }
    }
    for (index, function) in root.into_iter().enumerate() {
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
        let top_level_known = compact_function_top_level_known(
            program,
            declarations,
            bodies,
            source,
            Some(sources),
            function.namespace,
            function.id,
            Some(&functions),
        );
        let mut probe = CompactLowerConstructProbe {
            program,
            declarations,
            bodies,
            source,
            sources: Some(sources),
            current_namespace: function.namespace,
            functions: Some(&functions),
            top_level_known,
            output: CompactLowerConstructProbeOutput::default(),
            last_blocker_detail: None,
            strict_dynamic_methods: true,
            scratch: Rc::new(RefCell::new(BuildScratch::default())),
        };
        let (scc_member_count, scc_group) = scc_metadata[index];
        let unit = probe.lower_function_unit(
            function,
            dependencies[index].clone(),
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
    bodies: &CompactBodyProbeOutput,
    source: &str,
    sources: &SourceMap,
    functions: &LowerableFunctions<'_>,
    strict_dynamic_methods: bool,
) -> (ProgramBuild, CompactLowerConstructProbeOutput) {
    let mut probe = CompactLowerConstructProbe {
        program,
        declarations,
        bodies,
        source,
        sources: Some(sources),
        current_namespace: None,
        functions: Some(functions),
        top_level_known: compact_top_level_known(
            program,
            declarations,
            bodies,
            source,
            Some(sources),
            None,
            Some(functions),
        ),
        output: CompactLowerConstructProbeOutput::default(),
        last_blocker_detail: None,
        strict_dynamic_methods,
        scratch: Rc::new(RefCell::new(BuildScratch::default())),
    };
    let root = program.statement_ids().collect::<Vec<_>>();
    let lowered = probe.lower_program_statements(&root);
    (lowered, probe.output)
}

fn compact_top_level_known(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    bodies: &CompactBodyProbeOutput,
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
        bodies,
        source,
        sources,
        current_namespace: namespace,
        functions,
        top_level_known: FxHashMap::default(),
        output: CompactLowerConstructProbeOutput::default(),
        last_blocker_detail: None,
        strict_dynamic_methods: true,
        scratch: Rc::new(RefCell::new(BuildScratch::default())),
    };
    probe.collect_top_level_known(&statements)
}

fn compact_function_top_level_known(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    bodies: &CompactBodyProbeOutput,
    source: &str,
    sources: Option<&SourceMap>,
    namespace: Option<Name>,
    function_id: FunctionDefId,
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
        bodies,
        source,
        sources,
        current_namespace: namespace,
        functions,
        top_level_known: FxHashMap::default(),
        output: CompactLowerConstructProbeOutput::default(),
        last_blocker_detail: None,
        strict_dynamic_methods: true,
        scratch: Rc::new(RefCell::new(BuildScratch::default())),
    };
    let mut known = top_level_known_with_runtime_bindings();
    for stmt in statements {
        if compact_stmt_contains_function_def(program, stmt, function_id) {
            break;
        }
        probe.record_top_level_binding(stmt, &mut known);
    }
    known
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
        ArenaStmtKind::PureDef(id) | ArenaStmtKind::ProcDef(id) | ArenaStmtKind::StreamDef(id) => {
            id == function_id
        }
        _ => false,
    }
}

#[derive(Clone, Copy)]
struct CompactFunctionDef {
    key: LoweredFunctionKey,
    id: FunctionDefId,
    pure: bool,
    namespace: Option<Name>,
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
    program: &ArenaProgram,
    function: CompactFunctionDef,
) -> Vec<LoweredFunctionKey> {
    let functions = compact_function_defs(program);
    let index_of = functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.key, index))
        .collect::<FxHashMap<_, _>>();
    compact_function_call_edges(program, function.id, function.namespace, &index_of)
        .into_iter()
        .map(|index| functions[index].key)
        .collect()
}

fn compact_function_scc_metadata(
    program: &ArenaProgram,
    function: CompactFunctionDef,
) -> (usize, Option<usize>) {
    let functions = compact_function_defs(program)
        .into_iter()
        .filter(|candidate| candidate.pure == function.pure)
        .collect::<Vec<_>>();
    let Some(function_index) = functions
        .iter()
        .position(|candidate| candidate.key == function.key)
    else {
        return (1, None);
    };
    let index_of = functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.key, index))
        .collect::<FxHashMap<_, _>>();
    let adjacency = functions
        .iter()
        .map(|function| {
            compact_function_call_edges(program, function.id, function.namespace, &index_of)
        })
        .collect::<Vec<_>>();
    for (group, scc) in compact_tarjan_sccs(adjacency).into_iter().enumerate() {
        if scc.contains(&function_index) {
            return if scc.len() > 1 {
                (scc.len(), Some(group))
            } else {
                (1, None)
            };
        }
    }
    (1, None)
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
        ArenaStmtKind::Export(inner) => {
            compact_collect_stmt_call_edges(program, inner, namespace, index_of, edges)
        }
        ArenaStmtKind::Let { initializer, .. }
        | ArenaStmtKind::Var { initializer, .. }
        | ArenaStmtKind::Defer(initializer)
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
        ArenaStmtKind::GuardedStmt {
            stmt, condition, ..
        } => {
            compact_collect_stmt_call_edges(program, stmt, namespace, index_of, edges);
            compact_collect_expr_call_edges(program, condition, namespace, index_of, edges);
        }
        ArenaStmtKind::Break { value: Some(value) } | ArenaStmtKind::Expr(value) => {
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
                    | ArenaCallArgKind::Named { value: expr, .. } => {
                        compact_collect_expr_call_edges(program, expr, namespace, index_of, edges);
                    }
                }
            }
        }
        ArenaExprKind::List(items) => {
            for item in program.arena.expr_ids(items) {
                compact_collect_expr_call_edges(program, item, namespace, index_of, edges);
            }
        }
        ArenaExprKind::Record(fields) => {
            for field in program.arena.record_fields(fields) {
                match field.kind {
                    ArenaRecordFieldKind::Named { value, .. }
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
        ArenaExprKind::Match { value, arms } => {
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
        | ArenaExprKind::Field { base: expr, .. }
        | ArenaExprKind::NullSafeField { base: expr, .. } => {
            compact_collect_expr_call_edges(program, expr, namespace, index_of, edges);
        }
        ArenaExprKind::Binary { left, right, .. }
        | ArenaExprKind::Index {
            base: left,
            index: right,
        } => {
            compact_collect_expr_call_edges(program, left, namespace, index_of, edges);
            compact_collect_expr_call_edges(program, right, namespace, index_of, edges);
        }
        ArenaExprKind::Slice { base, start, end } => {
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
        ArenaExprKind::ListComp {
            expr,
            iter,
            condition,
            ..
        } => {
            compact_collect_expr_call_edges(program, expr, namespace, index_of, edges);
            compact_collect_expr_call_edges(program, iter, namespace, index_of, edges);
            if let Some(condition) = condition {
                compact_collect_expr_call_edges(program, condition, namespace, index_of, edges);
            }
        }
        ArenaExprKind::MapComp {
            key,
            value,
            iter,
            condition,
            ..
        } => {
            compact_collect_expr_call_edges(program, key, namespace, index_of, edges);
            compact_collect_expr_call_edges(program, value, namespace, index_of, edges);
            compact_collect_expr_call_edges(program, iter, namespace, index_of, edges);
            if let Some(condition) = condition {
                compact_collect_expr_call_edges(program, condition, namespace, index_of, edges);
            }
        }
        ArenaExprKind::Loop { block } | ArenaExprKind::Retry { block, .. } => {
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        ArenaExprKind::BuilderCall { call, .. } => {
            compact_collect_expr_call_edges(program, call, namespace, index_of, edges);
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
            });
        }
        ArenaStmtKind::ProcDef(def) => {
            let name = program.arena.function_def(def).name;
            functions.push(CompactFunctionDef {
                key: compact_function_key(namespace, name),
                id: def,
                pure: false,
                namespace,
            });
        }
        ArenaStmtKind::StreamDef(def) => {
            let name = program.arena.function_def(def).name;
            functions.push(CompactFunctionDef {
                key: compact_function_key(namespace, name),
                id: def,
                pure: true,
                namespace,
            });
        }
        _ => {}
    }
}

struct CompactLowerConstructProbe<'a, 'defs> {
    program: &'a ArenaProgram,
    declarations: &'a CompactDeclOutput,
    bodies: &'a CompactBodyProbeOutput,
    source: &'a str,
    sources: Option<&'a SourceMap>,
    current_namespace: Option<Name>,
    functions: Option<&'a LowerableFunctions<'defs>>,
    top_level_known: FxHashMap<Name, LoweredTopLevelBinding>,
    output: CompactLowerConstructProbeOutput,
    last_blocker_detail: Option<(Span, String)>,
    strict_dynamic_methods: bool,
    scratch: Rc<RefCell<BuildScratch>>,
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
        ArenaTypeExprTag::Named => 0,
        ArenaTypeExprTag::Qualified => 1,
        ArenaTypeExprTag::List => 2,
        ArenaTypeExprTag::Map => 3,
        ArenaTypeExprTag::Stream => 4,
        ArenaTypeExprTag::Module => 5,
        ArenaTypeExprTag::Result => 6,
        ArenaTypeExprTag::Optional => 7,
    }
}

fn compact_stmt_kind_index(kind: ArenaStmtKind) -> usize {
    match kind {
        ArenaStmtKind::Use(_) => 0,
        ArenaStmtKind::Export(_) => 1,
        ArenaStmtKind::TypeDef(_) => 2,
        ArenaStmtKind::ErrorDef(_) => 3,
        ArenaStmtKind::Let { .. } => 4,
        ArenaStmtKind::Var { .. } => 5,
        ArenaStmtKind::Assign { .. } => 6,
        ArenaStmtKind::ProcDef(_) => 7,
        ArenaStmtKind::PureDef(_) => 8,
        ArenaStmtKind::StreamDef(_) => 9,
        ArenaStmtKind::SignalHook(_) => 10,
        ArenaStmtKind::Return(_) => 11,
        ArenaStmtKind::Yield(_) => 12,
        ArenaStmtKind::Defer(_) => 13,
        ArenaStmtKind::If { .. } => 14,
        ArenaStmtKind::While { .. } => 15,
        ArenaStmtKind::For { .. } => 16,
        ArenaStmtKind::With { .. } => 17,
        ArenaStmtKind::Loop { .. } => 18,
        ArenaStmtKind::Guard { .. } => 19,
        ArenaStmtKind::GuardedStmt { .. } => 20,
        ArenaStmtKind::Break { .. } => 21,
        ArenaStmtKind::Continue => 22,
        ArenaStmtKind::Match { .. } => 23,
        ArenaStmtKind::Command(_) => 24,
        ArenaStmtKind::TailBareIdent(_) => 25,
        ArenaStmtKind::Expr(_) => 26,
    }
}

fn compact_stmt_kind_label(kind: ArenaStmtKind) -> &'static str {
    match kind {
        ArenaStmtKind::Use(_) => "use",
        ArenaStmtKind::Export(_) => "export",
        ArenaStmtKind::TypeDef(_) => "type_def",
        ArenaStmtKind::ErrorDef(_) => "error_def",
        ArenaStmtKind::Let { .. } => "let",
        ArenaStmtKind::Var { .. } => "var",
        ArenaStmtKind::Assign { .. } => "assign",
        ArenaStmtKind::ProcDef(_) => "proc_def",
        ArenaStmtKind::PureDef(_) => "pure_def",
        ArenaStmtKind::StreamDef(_) => "stream_def",
        ArenaStmtKind::SignalHook(_) => "signal_hook",
        ArenaStmtKind::Return(_) => "return",
        ArenaStmtKind::Yield(_) => "yield",
        ArenaStmtKind::Defer(_) => "defer",
        ArenaStmtKind::If { .. } => "if",
        ArenaStmtKind::While { .. } => "while",
        ArenaStmtKind::For { .. } => "for",
        ArenaStmtKind::With { .. } => "with",
        ArenaStmtKind::Loop { .. } => "loop",
        ArenaStmtKind::Guard { .. } => "guard",
        ArenaStmtKind::GuardedStmt { .. } => "guarded_stmt",
        ArenaStmtKind::Break { .. } => "break",
        ArenaStmtKind::Continue => "continue",
        ArenaStmtKind::Match { .. } => "match",
        ArenaStmtKind::Command(_) => "command",
        ArenaStmtKind::TailBareIdent(_) => "tail_bare_ident",
        ArenaStmtKind::Expr(_) => "expr",
    }
}

fn compact_stmt_blocker_label(program: &ArenaProgram, stmt: StmtId) -> String {
    match program.arena.stmt(stmt).kind {
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
        ArenaExprKind::Match { .. } => 19,
        ArenaExprKind::Unary { .. } => 20,
        ArenaExprKind::Binary { .. } => 21,
        ArenaExprKind::Call { .. } => 22,
        ArenaExprKind::Field { .. } => 23,
        ArenaExprKind::NullSafeField { .. } => 24,
        ArenaExprKind::Index { .. } => 25,
        ArenaExprKind::Slice { .. } => 26,
        ArenaExprKind::EnvGet { .. } => 27,
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
        ArenaExprKind::Retry { .. } => 38,
    }
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
        ArenaExprKind::Record(_) => "record",
        ArenaExprKind::If { .. } => "if",
        ArenaExprKind::Match { .. } => "match",
        ArenaExprKind::Unary { .. } => "unary",
        ArenaExprKind::Binary { .. } => "binary",
        ArenaExprKind::Call { .. } => "call",
        ArenaExprKind::Field { .. } => "field",
        ArenaExprKind::NullSafeField { .. } => "null_safe_field",
        ArenaExprKind::Index { .. } => "index",
        ArenaExprKind::Slice { .. } => "slice",
        ArenaExprKind::EnvGet { .. } => "env_get",
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
        ArenaExprKind::Retry { .. } => "retry",
    }
}

fn compact_checked_type_is_concrete(ty: &Type) -> bool {
    !matches!(ty, Type::Any | Type::Unknown | Type::Invalid) && !ty.contains_any()
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
        CompactErrorFamilyKey::Qualified(name) => name.to_string(),
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
            ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Var { target, .. } => {
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

fn lower_const_param_default(
    arena: &AstArena,
    expr: ExprId,
    kind: LoweredType,
) -> Option<LoweredValue> {
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
            for item in arena.expr_ids(items) {
                values.push(lower_const_param_default(arena, item, LoweredType::Any)?);
            }
            LoweredValue::List(values)
        }
        ArenaExprKind::Record(fields) => {
            let mut values = BTreeMap::new();
            for field in arena.record_fields(fields) {
                match field.kind {
                    ArenaRecordFieldKind::Named { name, value, .. } => {
                        values.insert(
                            Arc::<str>::from(name.as_str().as_str()),
                            lower_const_param_default(arena, value, LoweredType::Any)?,
                        );
                    }
                    ArenaRecordFieldKind::Spread { expr, .. } => {
                        let spread = lower_const_param_default(arena, expr, LoweredType::Any)?;
                        match spread {
                            LoweredValue::Record(spread) => values.extend(spread),
                            LoweredValue::RecordVec(spread) => {
                                for (name, value) in spread {
                                    values.insert(Arc::<str>::from(name.as_str().as_str()), value);
                                }
                            }
                            _ => return None,
                        }
                    }
                    ArenaRecordFieldKind::Shorthand { .. } => return None,
                }
            }
            LoweredValue::Record(values)
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
        .map(|stmt| compact_stmt_kind_index(program.arena.stmt(stmt).kind))
        .unwrap_or(COMPACT_STMT_KIND_COUNT - 1)
}

fn compact_body_tail_call_blocker_callee(program: &ArenaProgram, block: BlockId) -> Option<ExprId> {
    let stmt = program
        .arena
        .stmt_ids(program.arena.block(block).statements)
        .last()?;
    match program.arena.stmt(stmt).kind {
        ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr))) | ArenaStmtKind::Expr(expr) => {
            compact_expr_call_blocker_callee(program, expr)
        }
        ArenaStmtKind::Let {
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
    match program.arena.stmt(stmt).kind {
        ArenaStmtKind::Command(command) => Some(command),
        _ => None,
    }
}

const _: [(); COMPACT_TYPE_EXPR_TAG_COUNT] = [(); 8];
const _: [(); COMPACT_STMT_KIND_COUNT] = [(); 27];
const _: [(); COMPACT_EXPR_KIND_COUNT] = [(); 39];
const _: [(); COMPACT_CALL_BLOCKER_KIND_COUNT] = [(); 6];
const _: [(); COMPACT_COMMAND_BLOCKER_KIND_COUNT] = [(); 6];

fn stream_item_type(ty: &Type) -> Option<&Type> {
    match ty {
        Type::List(item) | Type::Stream(item) => Some(item.as_ref()),
        _ => None,
    }
}

impl CompactLowerConstructProbe<'_, '_> {
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
            .filter(|(name, binding)| binding.slot && slots.resolve(**name).is_none())
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
                let blocker = self.top_level_blocker_kind(*stmt, &known);
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
                        self.bodies,
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
                };
                let dependencies = compact_function_dependency_keys(self.program, function);
                let (scc_member_count, scc_group) =
                    compact_function_scc_metadata(self.program, function);
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
            ArenaStmtKind::ProcDef(def) => {
                self.output.functions += 1;
                let previous_known = std::mem::replace(
                    &mut self.top_level_known,
                    compact_function_top_level_known(
                        self.program,
                        self.declarations,
                        self.bodies,
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
                };
                let dependencies = compact_function_dependency_keys(self.program, function);
                let (scc_member_count, scc_group) =
                    compact_function_scc_metadata(self.program, function);
                let unit =
                    self.lower_function_unit(function, dependencies, scc_member_count, scc_group);
                if unit.is_lowered() {
                    self.output.constructed_functions += 1;
                    if self.program.arena.function_def(def).name == Name::intern("main") {
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
        let return_kind = match self.lowered_return_kind(def.return_ty) {
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
        for param in self.program.arena.params(def.params) {
            let kind = match lowered_arena_type(&self.program.arena, param.ty, self.declarations) {
                Some(kind) => kind,
                None => {
                    self.last_blocker_detail = Some((
                        self.program.arena.type_expr_span(param.ty),
                        format!("unsupported parameter type for `{}`", param.name.as_str()),
                    ));
                    return Err(CompactFunctionBlocker::ParamType);
                }
            };
            let default = match param.default {
                Some(expr) => match lower_const_param_default(&self.program.arena, expr, kind) {
                    Some(default) => Some(default),
                    None => {
                        self.last_blocker_detail = Some((
                            self.program.arena.expr(expr).span,
                            format!(
                                "unsupported default value for parameter `{}`",
                                param.name.as_str()
                            ),
                        ));
                        return Err(CompactFunctionBlocker::ParamDefault);
                    }
                },
                None => None,
            };
            param_kinds.push(kind);
            param_checks.push(compact_type_check(
                kind,
                &self.program.arena,
                param.ty,
                self.declarations,
            ));
            param_rest.push(param.rest);
            param_defaults.push(default);
            params.push(param.name);
        }
        if !self.program.arena.block(def.body).params.is_empty() {
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
        let mut slots = SlotScope::from_names(params.iter().copied());
        let captures = self.append_immutable_top_level_captures(&mut slots);
        let blockers_before = self.output.blocker_events;
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
            slot_count: slots.count(),
            body,
            has_defers,
            scratch: self.scratch.clone(),
        })
    }

    fn record_top_level_blocker_detail(&mut self, id: StmtId, blocker: CompactTopLevelBlocker) {
        if let ArenaStmtKind::Export(inner) = self.program.arena.stmt(id).kind {
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

    fn top_level_blocker_kind(
        &self,
        id: StmtId,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> CompactTopLevelBlocker {
        match self.program.arena.stmt(id).kind {
            ArenaStmtKind::Export(inner) => self.top_level_blocker_kind(inner, known),
            ArenaStmtKind::Use(_) => CompactTopLevelBlocker::Use,
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(value),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(value),
            } => {
                if simple_binding_target(self.program, target).is_none() {
                    return CompactTopLevelBlocker::BindingTarget;
                }
                if self.top_level_binding_kind(ty, value, known).is_none() {
                    return CompactTopLevelBlocker::BindingType;
                }
                CompactTopLevelBlocker::BindingExpression
            }
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            } => {
                if simple_binding_target(self.program, target).is_none() {
                    return CompactTopLevelBlocker::BindingTarget;
                }
                if self.top_level_run_binding_kind(ty, run).is_none() {
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
            ArenaStmtKind::If { .. }
            | ArenaStmtKind::While { .. }
            | ArenaStmtKind::For { .. }
            | ArenaStmtKind::Match { .. } => CompactTopLevelBlocker::Control,
            ArenaStmtKind::Command(_) => CompactTopLevelBlocker::Command,
            ArenaStmtKind::Expr(_) => CompactTopLevelBlocker::Expression,
            ArenaStmtKind::Defer(_) => CompactTopLevelBlocker::Defer,
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
            ArenaStmtKind::Export(inner) => self.lower_top_level_stmt(inner, known),
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
                            self.bodies,
                            self.source,
                            self.sources,
                            Some(module.name),
                            self.functions,
                        ),
                        output: CompactLowerConstructProbeOutput::default(),
                        last_blocker_detail: None,
                        strict_dynamic_methods: true,
                        scratch: self.scratch.clone(),
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
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: initializer @ ArenaExprOrRun::Expr(value),
            } => {
                if let ArenaBindingTargetKind::Record { fields, .. } =
                    self.program.arena.binding_target(target).kind
                {
                    let mut slots = top_level_slots(known);
                    let source = self.lower_expr(value, &mut slots, None, None)?;
                    let field_names = self
                        .program
                        .arena
                        .destructure_fields(fields)
                        .iter()
                        .map(|field| field.name)
                        .collect::<Vec<_>>();
                    return Some(lowered_top_level(
                        &self.scratch,
                        BuildTopKind::LetRecord {
                            source,
                            fields: field_names,
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
                let _kind = self.top_level_binding_kind(ty, value, known);
                let annotation = ty;
                let (ty, validation) = match ty {
                    Some(ty) => {
                        let lowered =
                            lowered_arena_type(&self.program.arena, ty, self.declarations)?;
                        if !lowerable_top_level_annotation(lowered) {
                            return None;
                        }
                        (
                            Some(lowered),
                            compact_type_check(lowered, &self.program.arena, ty, self.declarations),
                        )
                    }
                    None => (None, None),
                };
                let mut slots = top_level_slots(known);
                let value = if self.is_empty_record_in_map_context(value, annotation) {
                    push_build_row!(self, expr, BuildExprRow::EmptyMap)
                } else {
                    self.lower_expr(value, &mut slots, None, None)?
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
                let _run_kind = self.top_level_run_binding_kind(ty, run);
                let (ty, validation) = match ty {
                    Some(ty) => {
                        let lowered =
                            lowered_arena_type(&self.program.arena, ty, self.declarations)?;
                        if !lowerable_top_level_annotation(lowered) {
                            return None;
                        }
                        (
                            Some(lowered),
                            compact_type_check(lowered, &self.program.arena, ty, self.declarations),
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
                    let root = self.assign_target_root_name(target)?;
                    if !known.get(&root).is_some_and(|binding| binding.mutable) {
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
            ArenaStmtKind::If { .. }
            | ArenaStmtKind::While { .. }
            | ArenaStmtKind::For { .. }
            | ArenaStmtKind::Match { .. }
            | ArenaStmtKind::Loop { .. } => {
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
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Expr(value),
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Defer(ArenaExprOrRun::Expr(value)) => {
                let mut slots = top_level_slots(known);
                let value = self.lower_expr(value, &mut slots, None, None)?;
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Defer {
                        value,
                        span: self.program.arena.stmt(id).span,
                    },
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Defer(ArenaExprOrRun::Run(run)) => {
                let mut slots = top_level_slots(known);
                let value = self.lower_run_binding_value(run, &mut slots, None, None)?;
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Defer {
                        value,
                        span: self.program.arena.stmt(id).span,
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
                                initializer: ArenaExprOrRun::Expr(value),
                            }
                            | ArenaStmtKind::Var {
                                target,
                                ty,
                                initializer: ArenaExprOrRun::Expr(value),
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
                                        kind: self
                                            .top_level_binding_kind(ty, value, known)
                                            .unwrap_or(LoweredType::Any),
                                        result_ok: None,
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
                                        result_ok: None,
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
                                        result_ok: None,
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
                        result_ok: None,
                        checked: None,
                        mutable: false,
                        slot: true,
                    },
                );
            }
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(value),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(value),
            } => {
                if let ArenaBindingTargetKind::Record { fields, .. } =
                    self.program.arena.binding_target(target).kind
                {
                    let mutable = matches!(stmt.kind, ArenaStmtKind::Var { .. });
                    for field in self.program.arena.destructure_fields(fields) {
                        known.insert(
                            field.name,
                            LoweredTopLevelBinding {
                                kind: LoweredType::Any,
                                result_ok: None,
                                checked: None,
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
                if is_discard_name(name) {
                    return;
                }
                let kind = self
                    .top_level_binding_kind(ty, value, known)
                    .unwrap_or(LoweredType::Any);
                known.insert(
                    name,
                    LoweredTopLevelBinding {
                        kind,
                        result_ok: ty
                            .and_then(|ty| {
                                lowered_arena_result_ok_type(
                                    &self.program.arena,
                                    ty,
                                    self.declarations,
                                )
                            })
                            .or_else(|| self.infer_lowered_expr_result_ok_type(value, known)),
                        checked: self.top_level_binding_checked_type(ty, value, known),
                        mutable: matches!(stmt.kind, ArenaStmtKind::Var { .. }),
                        slot: true,
                    },
                );
            }
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            } => {
                let Some(name) = simple_binding_target(self.program, target) else {
                    return;
                };
                if is_discard_name(name) {
                    return;
                }
                let Some(kind) = self.top_level_run_binding_kind(ty, run) else {
                    return;
                };
                known.insert(
                    name,
                    LoweredTopLevelBinding {
                        kind,
                        result_ok: ty
                            .and_then(|ty| {
                                lowered_arena_result_ok_type(
                                    &self.program.arena,
                                    ty,
                                    self.declarations,
                                )
                            })
                            .or_else(|| self.infer_lowered_run_result_ok_type(run)),
                        checked: self.top_level_run_binding_checked_type(ty, run),
                        mutable: matches!(stmt.kind, ArenaStmtKind::Var { .. }),
                        slot: true,
                    },
                );
            }
            _ => {}
        }
    }

    fn top_level_binding_kind(
        &self,
        ty: Option<TypeExprId>,
        value: ExprId,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<LoweredType> {
        if let Some(ty) = ty {
            return lowered_arena_type(&self.program.arena, ty, self.declarations);
        }
        self.infer_checked_expr_type(value, known)
            .as_ref()
            .and_then(lowered_checked_type)
            .or_else(|| self.infer_lowered_expr_type(value, known))
    }

    fn top_level_binding_checked_type(
        &self,
        ty: Option<TypeExprId>,
        value: ExprId,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<Type> {
        ty.map(|ty| compact_runtime_type(&self.program.arena, ty, self.declarations))
            .or_else(|| self.infer_checked_expr_type(value, known))
    }

    fn infer_checked_expr_type(
        &self,
        value: ExprId,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<Type> {
        self.bodies
            .expr_types
            .get(&value)
            .filter(|ty| {
                compact_checked_type_is_concrete(ty)
                    && !matches!(
                        ty.result_ok(),
                        Some(Type::Any | Type::Unknown | Type::Invalid)
                    )
            })
            .cloned()
            .or_else(|| match self.program.arena.expr(value).kind {
                ArenaExprKind::Ident(name) => {
                    known.get(&name).and_then(|binding| binding.checked.clone())
                }
                ArenaExprKind::Field { base, name }
                | ArenaExprKind::NullSafeField { base, name } => {
                    self.infer_checked_field_type(base, name, known)
                }
                ArenaExprKind::Call { callee, args } => {
                    self.infer_checked_call_type(callee, args, known)
                }
                ArenaExprKind::Try(expr) => self.infer_checked_try_type(expr, known),
                ArenaExprKind::Spawn(_) => Some(Type::Result(
                    Box::new(Type::ProcessHandle),
                    Box::new(Type::ProcessError),
                )),
                ArenaExprKind::EnvGet { kind, .. } => match kind {
                    EnvGetKind::Str => Some(Type::Str),
                    EnvGetKind::Path => Some(Type::Path),
                    EnvGetKind::PathList => Some(Type::EnvPathList),
                },
                ArenaExprKind::EnvPathList => Some(Type::EnvPathList),
                _ => None,
            })
    }

    fn lower_binding_checked_type(
        &self,
        ty: Option<TypeExprId>,
        value: ExprId,
        slots: &SlotScope,
    ) -> Option<Type> {
        let expected =
            ty.map(|ty| compact_runtime_type(&self.program.arena, ty, self.declarations));
        let table_type = self
            .bodies
            .expr_types
            .get(&value)
            .filter(|ty| !matches!(ty, Type::Invalid))
            .cloned();
        let actual = self
            .infer_checked_expr_type_with_slots(value, slots)
            .or_else(|| self.infer_checked_expr_type(value, &self.top_level_known))
            .or_else(|| {
                table_type
                    .as_ref()
                    .filter(|ty| compact_checked_type_is_concrete(ty))
                    .cloned()
            })
            .or_else(|| {
                self.infer_lowered_expr_type(value, &self.top_level_known)
                    .and_then(type_for_lowered_type)
            })
            .or(table_type);
        match (expected, actual) {
            (Some(expected), _) => Some(expected),
            (None, actual) => actual,
        }
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
            return Some(lowered);
        };
        let kind = self
            .infer_lowered_expr_type(value, &self.top_level_known)
            .unwrap_or(LoweredType::Any);
        if lowered_type_needs_static_check(kind) {
            let check = LoweredTypeCheck {
                ty: checked_ty.cloned().unwrap_or_else(|| {
                    compact_runtime_type(&self.program.arena, ty, self.declarations)
                }),
                name: compact_type_expr_name(&self.program.arena, ty),
            };
            Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Try(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Require {
                        value: lowered,
                        check,
                        span,
                    }
                ))
            ))
        } else {
            Some(lowered)
        }
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
            return self.lowered_method_supported_for_type(ty, name, arg_count);
        }
        let Some(ty) = self
            .infer_checked_expr_type_with_slots(base, slots)
            .or_else(|| self.infer_checked_expr_type(base, &self.top_level_known))
        else {
            return true;
        };
        self.lowered_method_supported_for_type(&ty, name, arg_count)
    }

    fn lowered_method_supported_for_type(&self, ty: &Type, name: Name, arg_count: usize) -> bool {
        if !self.strict_dynamic_methods
            && matches!(ty, Type::Any | Type::Unknown)
            && lowered_method_name(&name.as_str())
        {
            return true;
        }
        lowered_method_supported_for_type(ty, name, arg_count)
    }

    fn infer_loop_item_checked_type(&self, iter: ExprId, slots: &SlotScope) -> Option<Type> {
        self.infer_checked_expr_type_with_slots(iter, slots)
            .or_else(|| self.infer_checked_expr_type(iter, &self.top_level_known))
            .and_then(|ty| match ty {
                Type::List(item) | Type::Stream(item) => Some(*item),
                _ => None,
            })
    }

    fn infer_checked_expr_type_with_slots(&self, value: ExprId, slots: &SlotScope) -> Option<Type> {
        match self.program.arena.expr(value).kind {
            ArenaExprKind::Ident(name) => slots.binding_type(name).cloned(),
            ArenaExprKind::Bool(_) => Some(Type::Bool),
            ArenaExprKind::Int(_) => Some(Type::Int),
            ArenaExprKind::Float(_) => Some(Type::Float),
            ArenaExprKind::Str(_) | ArenaExprKind::FmtString(_) => Some(Type::Str),
            ArenaExprKind::PathStr(_) | ArenaExprKind::PathFmtString(_) => Some(Type::Path),
            ArenaExprKind::Null => Some(Type::Null),
            ArenaExprKind::Duration(_) => Some(Type::Duration),
            ArenaExprKind::Bytes(_) => Some(Type::Bytes),
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                let expected = self
                    .infer_checked_expr_type_with_slots(else_value, slots)
                    .or_else(|| self.infer_checked_expr_type(else_value, &self.top_level_known))?;
                for branch in self.program.arena.if_expr_branches(branches) {
                    let branch_ty = self
                        .infer_checked_expr_type_with_slots(branch.value, slots)
                        .or_else(|| {
                            self.infer_checked_expr_type(branch.value, &self.top_level_known)
                        });
                    if let Some(branch_ty) = branch_ty
                        && branch_ty != expected
                    {
                        return None;
                    }
                }
                Some(expected)
            }
            ArenaExprKind::Pipeline { input, stages } => {
                self.infer_checked_pipeline_type_with_slots(input, stages, slots)
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                self.infer_checked_structured_pipeline_type_with_slots(input, stages, slots)
            }
            ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name } => {
                if let Some(ty) = self.infer_checked_env_field_type(base, name) {
                    return Some(ty);
                }
                let base_ty = self
                    .infer_checked_expr_type_with_slots(base, slots)
                    .or_else(|| self.infer_checked_expr_type(base, &self.top_level_known))?;
                self.infer_checked_field_type_from_type(&base_ty, name)
            }
            ArenaExprKind::Index { base, .. } => {
                let base_ty = self
                    .infer_checked_expr_type_with_slots(base, slots)
                    .or_else(|| self.infer_checked_expr_type(base, &self.top_level_known))?;
                match base_ty {
                    Type::List(item) | Type::Map(item) => Some(*item),
                    _ => None,
                }
            }
            ArenaExprKind::Call { callee, args } => {
                let args_vec = self.program.arena.call_args(args);
                if let ArenaExprKind::Ident(name) = self.program.arena.expr(callee).kind {
                    if name == "Path" && single_positional_arena_call_arg(args_vec).is_some() {
                        return Some(Type::Path);
                    }
                    return self
                        .declarations
                        .pures
                        .get(&name)
                        .or_else(|| self.declarations.procs.get(&name))
                        .or_else(|| self.declarations.streams.get(&name))
                        .map(|sig| sig.return_ty.clone());
                }
                let (ArenaExprKind::Field { base, name }
                | ArenaExprKind::NullSafeField { base, name }) =
                    self.program.arena.expr(callee).kind
                else {
                    return None;
                };
                let base_ty = self.infer_checked_expr_type_with_slots(base, slots);
                if name == "require" {
                    let [arg] = args_vec else {
                        return None;
                    };
                    let contract = compact_call_arg_expr(arg)?;
                    let contract_ty = match self.program.arena.expr(contract).kind {
                        ArenaExprKind::Ident(name) => match self.declarations.types.get(&name) {
                            Some(CompactTypeDefInfo::Module(exports)) => {
                                Type::Module(exports.clone())
                            }
                            _ => self.infer_checked_expr_type(contract, &self.top_level_known)?,
                        },
                        _ => self.infer_checked_expr_type(contract, &self.top_level_known)?,
                    };
                    return Some(Type::Result(Box::new(contract_ty), Box::new(Type::Error)));
                }
                let base_ty = base_ty?;
                if name == "set" || name == "remove" || name == "push" {
                    return Some(base_ty);
                }
                if name == "keys" {
                    return Some(Type::List(Box::new(Type::Str)));
                }
                if name == "values" {
                    let item =
                        self.infer_checked_get_value_type(base, args, &self.top_level_known)?;
                    return Some(Type::List(Box::new(item)));
                }
                if name == "get" {
                    return match (&base_ty, args_vec.len()) {
                        (Type::List(item) | Type::Map(item), 1) => {
                            Some(Type::Result(item.clone(), Box::new(Type::Error)))
                        }
                        (Type::List(item) | Type::Map(item), 2) => {
                            let fallback = compact_call_arg_expr(&args_vec[1])?;
                            Some(
                                self.infer_checked_expr_type(fallback, &self.top_level_known)
                                    .or_else(|| {
                                        self.infer_checked_expr_type_with_slots(fallback, slots)
                                    })
                                    .unwrap_or_else(|| item.as_ref().clone()),
                            )
                        }
                        _ => None,
                    };
                }
                if !lowered_method_supported_for_type(&base_ty, name, args_vec.len()) {
                    if let Some(return_ty) = module_export_call_return_type(base_ty.clone(), name) {
                        return Some(return_ty);
                    }
                    return None;
                }
                infer_checked_method_return_type(&base_ty, name)
            }
            ArenaExprKind::Require { schema, .. } => {
                let contract_ty = match self.program.arena.type_expr_tags.get(schema.index()) {
                    Some(ArenaTypeExprTag::Named) => {
                        let name = Name::from_symbol(Symbol::from_raw(
                            self.program.arena.type_expr_data[schema.index()].lhs,
                        ));
                        match self.declarations.types.get(&name) {
                            Some(CompactTypeDefInfo::Module(exports)) => {
                                Type::Module(exports.clone())
                            }
                            _ => lowered_arena_type(&self.program.arena, schema, self.declarations)
                                .and_then(type_for_lowered_type)
                                .unwrap_or(Type::Any),
                        }
                    }
                    _ => lowered_arena_type(&self.program.arena, schema, self.declarations)
                        .and_then(type_for_lowered_type)
                        .unwrap_or(Type::Any),
                };
                Some(Type::Result(Box::new(contract_ty), Box::new(Type::Error)))
            }
            ArenaExprKind::List(items) => {
                let item_types: Vec<Type> = self
                    .program
                    .arena
                    .expr_ids(items)
                    .filter_map(|item| self.infer_checked_expr_type_with_slots(item, slots))
                    .map(|ty| ty.result_ok().cloned().unwrap_or(ty))
                    .collect();
                let first = item_types.first().cloned();
                let unified = item_types
                    .into_iter()
                    .reduce(|acc, ty| if acc == ty { acc } else { Type::Any })?;
                first
                    .filter(|first| unified == *first)
                    .or(Some(unified))
                    .map(|item_ty| Type::List(Box::new(item_ty)))
            }
            ArenaExprKind::ListComp {
                expr: value_expr,
                iter,
                ..
            } => {
                let item_ty = self
                    .infer_checked_expr_type_with_slots(value_expr, slots)
                    .or_else(|| {
                        let iter_ty = self
                            .infer_checked_expr_type_with_slots(iter, slots)
                            .or_else(|| {
                                self.infer_checked_expr_type(iter, &self.top_level_known)
                            })?;
                        match iter_ty {
                            Type::List(item) | Type::Stream(item) => Some(*item),
                            _ => None,
                        }
                    });
                item_ty
                    .map(|item_ty| Type::List(Box::new(item_ty)))
                    .or_else(|| Some(Type::List(Box::new(Type::Any))))
            }
            ArenaExprKind::MapComp { .. } => Some(Type::Map(Box::new(Type::Any))),
            ArenaExprKind::Try(expr) => self
                .infer_checked_expr_type_with_slots(expr, slots)
                .and_then(|ty| ty.result_ok().cloned()),
            ArenaExprKind::Run(run) => self
                .infer_lowered_run_binding_type(run)
                .and_then(type_for_lowered_type),
            _ => None,
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
        slots: &SlotScope,
    ) -> (Option<Type>, Option<Type>) {
        let scrutinee_ty = self
            .infer_checked_expr_type_with_slots(scrutinee, slots)
            .or_else(|| self.infer_checked_expr_type(scrutinee, &self.top_level_known));
        match scrutinee_ty {
            Some(Type::Result(ok, err)) => {
                let ok_ty = compact_checked_type_is_concrete(&ok).then(|| ok.as_ref().clone());
                let err_ty = compact_checked_type_is_concrete(&err).then(|| err.as_ref().clone());
                (ok_ty, err_ty)
            }
            _ => (None, None),
        }
    }

    fn infer_checked_pipeline_type_with_slots(
        &self,
        input: ExprId,
        stages: crate::syntax::arena::ArenaRange,
        slots: &SlotScope,
    ) -> Option<Type> {
        let mut current = self
            .infer_checked_expr_type(input, &self.top_level_known)
            .or_else(|| self.infer_checked_expr_type_with_slots(input, slots))?;
        current = current.result_ok().cloned().unwrap_or(current);
        for stage in self.program.arena.pipe_stages(stages) {
            let ArenaPipeStageKind::Stream(stage) = &stage.kind else {
                continue;
            };
            current = self.infer_checked_stream_stage_type_with_slots(&current, stage, slots)?;
        }
        Some(current)
    }

    fn infer_checked_structured_pipeline_type_with_slots(
        &self,
        input: ExprId,
        stages: crate::syntax::arena::ArenaRange,
        slots: &SlotScope,
    ) -> Option<Type> {
        let mut current = self
            .infer_checked_expr_type(input, &self.top_level_known)
            .or_else(|| self.infer_checked_expr_type_with_slots(input, slots))?;
        current = current.result_ok().cloned().unwrap_or(current);
        for stage in self.program.arena.stream_stages(stages) {
            current = self.infer_checked_stream_stage_type_with_slots(&current, stage, slots)?;
        }
        Some(current)
    }

    fn infer_checked_stream_stage_type_with_slots(
        &self,
        input: &Type,
        stage: &ArenaStreamStage,
        slots: &SlotScope,
    ) -> Option<Type> {
        match stage.kind {
            StreamStageKind::Map | StreamStageKind::ParMap => {
                let item = match input {
                    Type::List(item) | Type::Stream(item) => item.as_ref().clone(),
                    _ => return None,
                };
                let value = self.infer_checked_pipeline_stage_block_tail(stage, item, slots)?;
                let item = value.result_ok().cloned().unwrap_or(value);
                Some(Type::List(Box::new(item)))
            }
            StreamStageKind::FlatMap => {
                let item = match input {
                    Type::List(item) | Type::Stream(item) => item.as_ref().clone(),
                    _ => return None,
                };
                let value = self.infer_checked_pipeline_stage_block_tail(stage, item, slots)?;
                match value {
                    Type::List(item) | Type::Stream(item) => Some(Type::List(item)),
                    _ => None,
                }
            }
            StreamStageKind::Count => {
                if stage.block.is_some() {
                    Some(Type::Map(Box::new(Type::Int)))
                } else {
                    Some(Type::Int)
                }
            }
            StreamStageKind::ReduceBy => Some(Type::Map(Box::new(Type::Any))),
            StreamStageKind::Any | StreamStageKind::All => Some(Type::Bool),
            StreamStageKind::Sum => Some(Type::Int),
            StreamStageKind::Where
            | StreamStageKind::Sort
            | StreamStageKind::SortBy
            | StreamStageKind::UniqueBy
            | StreamStageKind::Take
            | StreamStageKind::Drop
            | StreamStageKind::Shuffle => match input {
                Type::List(item) | Type::Stream(item) => Some(Type::List(item.clone())),
                _ => None,
            },
            StreamStageKind::Batch => match input {
                Type::List(item) | Type::Stream(item) => {
                    Some(Type::List(Box::new(Type::List(item.clone()))))
                }
                _ => None,
            },
            StreamStageKind::Enumerate => {
                let value = match input {
                    Type::List(item) | Type::Stream(item) => item.as_ref().clone(),
                    _ => return None,
                };
                let mut fields = BTreeMap::new();
                fields.insert(Name::intern("index"), Type::Int);
                fields.insert(Name::intern("value"), value);
                Some(Type::List(Box::new(Type::Record(fields))))
            }
            StreamStageKind::First
            | StreamStageKind::Last
            | StreamStageKind::Min
            | StreamStageKind::Max => {
                // These terminals consume the stream and return a single item;
                // in this build `first`/`last`/`min`/`max` yield a `Result`, so
                // the pipeline carries a `Result<item, Error>` until a postfix
                // `?` (or null-safe receiver) unwraps it. Without this the
                // lightweight slot inference falls through to the input list
                // type, so a null-safe method call such as
                // `(s.split(".") |> last())?.lower()` is mistaken for a method
                // on the list and wrongly rejected as an IR blocker.
                let item = match input {
                    Type::List(item) | Type::Stream(item) => item.as_ref().clone(),
                    _ => return None,
                };
                Some(Type::Result(Box::new(item), Box::new(Type::Error)))
            }
            StreamStageKind::Collect => match input {
                Type::List(item) | Type::Stream(item) => Some(Type::List(item.clone())),
                _ => None,
            },
            _ => lowered_checked_type(input).and_then(type_for_lowered_type),
        }
    }

    fn infer_checked_pipeline_stage_block_tail(
        &self,
        stage: &ArenaStreamStage,
        item: Type,
        slots: &SlotScope,
    ) -> Option<Type> {
        let block = stage.block?;
        let ids = self
            .program
            .arena
            .stmt_ids(self.program.arena.block(block).statements)
            .collect::<Vec<_>>();
        let tail = *ids.last()?;
        let mut scoped = slots.clone();
        let saved = scoped.enter();
        match self
            .program
            .arena
            .block_params(self.program.arena.block(block).params)
        {
            [] => {}
            [param] => {
                if scoped.is_bound_non_capture(param.name) {
                    return None;
                }
                scoped.declare_with_type(param.name, Some(item));
            }
            _ => return None,
        }
        let ty = match self.program.arena.stmt(tail).kind {
            ArenaStmtKind::Expr(expr) => self.infer_checked_expr_type_with_slots(expr, &scoped),
            ArenaStmtKind::TailBareIdent(name) => scoped.binding_type(name).cloned(),
            _ => None,
        };
        scoped.exit(saved);
        ty
    }

    fn infer_checked_try_type(
        &self,
        expr: ExprId,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<Type> {
        self.bodies
            .expr_types
            .get(&expr)
            .and_then(|ty| ty.result_ok())
            .filter(|ty| compact_checked_type_is_concrete(ty))
            .cloned()
            .or_else(|| {
                self.infer_checked_expr_type(expr, known).and_then(|ty| {
                    ty.result_ok()
                        .filter(|ok| compact_checked_type_is_concrete(ok))
                        .cloned()
                })
            })
            .or_else(|| match self.program.arena.expr(expr).kind {
                ArenaExprKind::EnvGet { kind, .. } => match kind {
                    EnvGetKind::Str => Some(Type::Str),
                    EnvGetKind::Path => Some(Type::Path),
                    EnvGetKind::PathList => Some(Type::EnvPathList),
                },
                ArenaExprKind::EnvPathList => Some(Type::EnvPathList),
                ArenaExprKind::Call { callee, args } => self
                    .infer_lowered_call_ok_type(callee, args, known)
                    .and_then(type_for_lowered_type),
                ArenaExprKind::Spawn(_) => Some(Type::ProcessHandle),
                _ => None,
            })
    }

    fn infer_checked_call_type(
        &self,
        callee: ExprId,
        args: crate::syntax::arena::ArenaRange,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<Type> {
        let args_vec = self.program.arena.call_args(args);
        if let ArenaExprKind::Ident(name) = self.program.arena.expr(callee).kind
            && name == "Path"
            && single_positional_arena_call_arg(args_vec).is_some()
        {
            return Some(Type::Path);
        }
        let (ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name }) =
            self.program.arena.expr(callee).kind
        else {
            return None;
        };
        if name == "set" || name == "remove" || name == "push" {
            return self.infer_checked_expr_type(base, known);
        }
        if name == "keys" {
            return Some(Type::List(Box::new(Type::Str)));
        }
        if name == "values" {
            let item = self.infer_checked_get_value_type(base, args, known)?;
            return Some(Type::List(Box::new(item)));
        }
        if name == "get" {
            let value = self.infer_checked_get_value_type(base, args, known)?;
            return match args_vec.len() {
                1 => Some(Type::Result(Box::new(value), Box::new(Type::Error))),
                2 => {
                    let fallback = compact_call_arg_expr(&args_vec[1])?;
                    Some(
                        self.infer_checked_expr_type(fallback, known)
                            .unwrap_or(value),
                    )
                }
                _ => None,
            };
        }
        if name == "require" {
            let [arg] = args_vec else {
                return None;
            };
            let contract = compact_call_arg_expr(arg)?;
            let contract_ty = match self.program.arena.expr(contract).kind {
                ArenaExprKind::Ident(name) => match self.declarations.types.get(&name) {
                    Some(CompactTypeDefInfo::Module(exports)) => Type::Module(exports.clone()),
                    _ => self.infer_checked_expr_type(contract, known)?,
                },
                _ => self.infer_checked_expr_type(contract, known)?,
            };
            return Some(Type::Result(Box::new(contract_ty), Box::new(Type::Error)));
        }
        if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind {
            if module == "archive"
                && (name == "tar_list" || name == "cpio_list" || name == "zip_list")
                && let Some(entry) = standard_record_type("ArchiveEntry")
            {
                let collection = if name == "tar_list" {
                    Type::Stream(Box::new(entry.clone()))
                } else {
                    Type::List(Box::new(entry))
                };
                return Some(Type::Result(Box::new(collection), Box::new(Type::Error)));
            }
            if module == "fs"
                && (name == "files" || name == "walk" || name == "ls" || name == "children")
                && let Some(entry) = standard_record_type("FsEntry")
            {
                return Some(Type::List(Box::new(entry)));
            }
            if module == "bytes" && (name == "copy" || name == "copy_file") {
                let mut fields = BTreeMap::new();
                fields.insert(Name::intern("bytes"), Type::Int);
                fields.insert(Name::intern("blocks"), Type::Int);
                return Some(Type::Result(
                    Box::new(Type::Record(fields)),
                    Box::new(Type::Error),
                ));
            }
            if module == "Path" && name == "parse_bytes" {
                return Some(Type::Result(Box::new(Type::Path), Box::new(Type::Error)));
            }
        }
        if let Some(return_ty) = self
            .infer_checked_expr_type(base, known)
            .and_then(|ty| infer_checked_method_return_type(&ty, name))
        {
            return Some(return_ty);
        }
        if let Some(return_ty) = self
            .infer_checked_expr_type(base, known)
            .and_then(|ty| module_export_call_return_type(ty, name))
        {
            return Some(return_ty);
        }
        None
    }

    fn infer_checked_field_type(
        &self,
        base: ExprId,
        name: Name,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<Type> {
        if let Some(ty) = self.infer_checked_env_field_type(base, name) {
            return Some(ty);
        }
        self.infer_checked_field_type_from_type(&self.infer_checked_expr_type(base, known)?, name)
    }

    fn infer_checked_field_type_from_type(&self, base_ty: &Type, name: Name) -> Option<Type> {
        match base_ty {
            Type::Optional(inner) | Type::Result(inner, _) => {
                self.infer_checked_field_type_from_type(inner, name)
            }
            Type::Record(fields) => fields.get(&name).cloned(),
            Type::Module(exports) => exports.get(&name).map(ModuleExportType::field_type),
            Type::Path => match name.as_str().as_str() {
                "display" | "name" | "ext" => Some(Type::Str),
                "normalize" | "parent" => Some(Type::Path),
                _ => None,
            },
            _ => None,
        }
    }

    fn infer_checked_env_field_type(&self, base: ExprId, name: Name) -> Option<Type> {
        match self.program.arena.expr(base).kind {
            ArenaExprKind::Ident(base_name) if base_name == "env" => {
                if name == "PATH" {
                    Some(Type::EnvPathList)
                } else {
                    Some(Type::Result(Box::new(Type::Str), Box::new(Type::Error)))
                }
            }
            ArenaExprKind::Field {
                base: inner_base,
                name: type_name,
            } => {
                let ArenaExprKind::Ident(inner_name) = self.program.arena.expr(inner_base).kind
                else {
                    return None;
                };
                if inner_name != "env" {
                    return None;
                }
                let value = match type_name.as_str().as_str() {
                    "Path" => Type::Path,
                    "PathList" => Type::EnvPathList,
                    _ => Type::Str,
                };
                Some(Type::Result(Box::new(value), Box::new(Type::Error)))
            }
            _ => None,
        }
    }

    fn infer_checked_get_value_type(
        &self,
        base: ExprId,
        args: crate::syntax::arena::ArenaRange,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<Type> {
        match self.infer_checked_expr_type(base, known)? {
            Type::List(item) | Type::Map(item) => Some(*item),
            Type::Record(fields) => {
                let args = self.program.arena.call_args(args);
                let [arg, ..] = args else {
                    return None;
                };
                let key = compact_call_arg_expr(arg)?;
                let ArenaExprKind::Str(key) = self.program.arena.expr(key).kind else {
                    return None;
                };
                let key = Name::intern(self.program.arena.string_literal(key).as_ref());
                fields.get(&key).cloned()
            }
            _ => None,
        }
    }

    fn top_level_run_binding_kind(
        &self,
        ty: Option<TypeExprId>,
        run: crate::syntax::arena::RunFormId,
    ) -> Option<LoweredType> {
        if let Some(ty) = ty {
            return lowered_arena_type(&self.program.arena, ty, self.declarations);
        }
        self.infer_lowered_run_binding_type(run)
    }

    fn top_level_run_binding_checked_type(
        &self,
        ty: Option<TypeExprId>,
        run: crate::syntax::arena::RunFormId,
    ) -> Option<Type> {
        ty.map(|ty| compact_runtime_type(&self.program.arena, ty, self.declarations))
            .or_else(|| match self.infer_lowered_run_binding_type(run)? {
                LoweredType::Status => Some(Type::Status),
                LoweredType::Str => Some(Type::Str),
                LoweredType::Bytes => Some(Type::Bytes),
                _ => None,
            })
    }

    fn infer_lowered_run_binding_type(
        &self,
        run: crate::syntax::arena::RunFormId,
    ) -> Option<LoweredType> {
        let ok = lowered_arena_run_binding_type(&self.program.arena, run)?;
        if self.program.arena.run_form(run).propagate || ok == LoweredType::Status {
            Some(ok)
        } else {
            Some(LoweredType::Result)
        }
    }

    fn infer_lowered_run_result_ok_type(
        &self,
        run: crate::syntax::arena::RunFormId,
    ) -> Option<LoweredType> {
        (!self.program.arena.run_form(run).propagate)
            .then(|| lowered_arena_run_capture_type(&self.program.arena, run))
            .flatten()
    }

    // A `fold`/`reduce` terminal produces a scalar of the accumulator's type, not
    // a List. Infer it from the seed (the single positional arg) so downstream
    // references to the binding are typed correctly; fall back to None (untyped)
    // rather than the List default, which would poison scalar uses of the result.
    fn infer_fold_result_type(
        &self,
        stage: &ArenaStreamStage,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<LoweredType> {
        let [arg] = self.program.arena.call_args(stage.args) else {
            return None;
        };
        let ArenaCallArgKind::Positional(initial) = arg.kind else {
            return None;
        };
        self.infer_lowered_expr_type(initial, known)
    }

    fn infer_lowered_expr_type(
        &self,
        value: ExprId,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<LoweredType> {
        match self.program.arena.expr(value).kind {
            ArenaExprKind::Bool(_) => Some(LoweredType::Bool),
            ArenaExprKind::Int(_) => Some(LoweredType::Int),
            ArenaExprKind::Float(_) => Some(LoweredType::Float),
            ArenaExprKind::Duration(_) => Some(LoweredType::Duration),
            ArenaExprKind::Str(_) | ArenaExprKind::FmtString(_) => Some(LoweredType::Str),
            ArenaExprKind::PathStr(_) | ArenaExprKind::PathFmtString(_) => Some(LoweredType::Path),
            ArenaExprKind::Bytes(_) => Some(LoweredType::Bytes),
            ArenaExprKind::List(_) | ArenaExprKind::ListComp { .. } => Some(LoweredType::List),
            ArenaExprKind::MapComp { .. } => Some(LoweredType::Map),
            ArenaExprKind::Record(_) => Some(LoweredType::Record),
            ArenaExprKind::EnvGet { .. } | ArenaExprKind::EnvPathList => Some(LoweredType::Result),
            ArenaExprKind::Require { .. } => Some(LoweredType::Result),
            ArenaExprKind::Spawn(_) | ArenaExprKind::Wait(_) => Some(LoweredType::Result),
            ArenaExprKind::Ident(name) => {
                known.get(&name).map(|binding| binding.kind).or_else(|| {
                    (self.compact_tag_variant_arity(name) == Some(0)).then_some(LoweredType::Tag)
                })
            }
            // A value-producing `if` has the common type of its branches.
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                let expected = self.infer_lowered_expr_type(else_value, known)?;
                self.program
                    .arena
                    .if_expr_branches(branches)
                    .iter()
                    .all(|branch| {
                        self.infer_lowered_expr_type(branch.value, known) == Some(expected)
                    })
                    .then_some(expected)
            }
            ArenaExprKind::Match { arms, .. } => {
                let mut expected = None;
                for arm in self.program.arena.match_expr_arms(arms) {
                    let kind = self.infer_lowered_expr_type(arm.value, known)?;
                    if let Some(expected) = expected {
                        if expected != kind {
                            return None;
                        }
                    } else {
                        expected = Some(kind);
                    }
                }
                expected
            }
            ArenaExprKind::Binary { op, left, right } => {
                self.infer_lowered_binary_type(op, left, right, known)
            }
            ArenaExprKind::Pipeline { stages, .. } => {
                let pipe_stages = self.program.arena.pipe_stages(stages).to_vec();
                let last_stream = pipe_stages.iter().rev().find_map(|ps| match &ps.kind {
                    ArenaPipeStageKind::Stream(s) => Some(s),
                    ArenaPipeStageKind::Expr(_) => None,
                });
                match last_stream.map(|s| s.kind.clone()) {
                    Some(StreamStageKind::Count) => Some(LoweredType::Int),
                    Some(StreamStageKind::Any | StreamStageKind::All) => Some(LoweredType::Bool),
                    Some(StreamStageKind::Sum) => Some(LoweredType::Int),
                    Some(
                        StreamStageKind::First
                        | StreamStageKind::Last
                        | StreamStageKind::Min
                        | StreamStageKind::Max,
                    ) => Some(LoweredType::Result),
                    Some(StreamStageKind::Fold | StreamStageKind::Reduce) => {
                        last_stream.and_then(|stage| self.infer_fold_result_type(stage, known))
                    }
                    Some(StreamStageKind::ReduceBy) => Some(LoweredType::Map),
                    _ => Some(LoweredType::List),
                }
            }
            ArenaExprKind::StructuredPipeline { stages, .. } => {
                let stages = self.program.arena.stream_stages(stages).to_vec();
                match stages.last() {
                    Some(stage) if stage.kind == StreamStageKind::Count => {
                        if stage.block.is_some() {
                            Some(LoweredType::Map)
                        } else {
                            Some(LoweredType::Int)
                        }
                    }
                    Some(stage)
                        if matches!(stage.kind, StreamStageKind::Any | StreamStageKind::All) =>
                    {
                        Some(LoweredType::Bool)
                    }
                    Some(stage) if stage.kind == StreamStageKind::Sum => Some(LoweredType::Int),
                    Some(stage)
                        if matches!(
                            stage.kind,
                            StreamStageKind::First
                                | StreamStageKind::Last
                                | StreamStageKind::Min
                                | StreamStageKind::Max
                        ) =>
                    {
                        Some(LoweredType::Result)
                    }
                    Some(stage)
                        if matches!(
                            stage.kind,
                            StreamStageKind::Fold | StreamStageKind::Reduce
                        ) =>
                    {
                        self.infer_fold_result_type(stage, known)
                    }
                    Some(stage) if stage.kind == StreamStageKind::ReduceBy => {
                        Some(LoweredType::Map)
                    }
                    _ => Some(LoweredType::List),
                }
            }
            ArenaExprKind::Try(expr) => self
                .bodies
                .expr_types
                .get(&value)
                .filter(|ty| compact_checked_type_is_concrete(ty))
                .and_then(lowered_checked_type)
                .or_else(|| self.infer_lowered_try_type(expr, known)),
            ArenaExprKind::Run(run) => self.infer_lowered_run_binding_type(run),
            ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name } => {
                self.bodies
                    .expr_types
                    .get(&value)
                    .and_then(lowered_checked_type)
                    .or_else(|| {
                        self.infer_checked_field_type(base, name, known)
                            .as_ref()
                            .and_then(lowered_checked_type)
                    })
            }
            ArenaExprKind::Slice { base, .. } => match self.infer_lowered_expr_type(base, known)? {
                kind @ (LoweredType::List | LoweredType::Str | LoweredType::Bytes) => Some(kind),
                _ => self
                    .bodies
                    .expr_types
                    .get(&value)
                    .and_then(lowered_checked_type),
            },
            ArenaExprKind::Call { callee, args } => {
                self.infer_lowered_call_expr_type(callee, args, known)
            }
            _ => self
                .bodies
                .expr_types
                .get(&value)
                .and_then(lowered_checked_type),
        }
    }

    fn infer_lowered_call_expr_type(
        &self,
        callee: ExprId,
        args: crate::syntax::arena::ArenaRange,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<LoweredType> {
        let args_vec = self.program.arena.call_args(args);
        if let ArenaExprKind::Ident(name) = self.program.arena.expr(callee).kind
            && name == "Path"
            && single_positional_arena_call_arg(args_vec).is_some()
        {
            return Some(LoweredType::Path);
        }
        if let ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name } =
            self.program.arena.expr(callee).kind
        {
            if name == "get" {
                return match args_vec.len() {
                    1 => Some(LoweredType::Result),
                    2 => {
                        let fallback = compact_call_arg_expr(&args_vec[1])?;
                        self.infer_checked_expr_type(fallback, known)
                            .as_ref()
                            .and_then(lowered_checked_type)
                            .or_else(|| self.infer_lowered_expr_type(fallback, known))
                    }
                    _ => None,
                };
            }
            if name == "set" {
                return Some(LoweredType::Map);
            }
            if lowered_result_method_ok_type(name).is_some() {
                return Some(LoweredType::Result);
            }
            if let Some(kind) = lowered_plain_method_type(name) {
                return Some(kind);
            }
            if let Some(kind) = self
                .infer_checked_expr_type(base, known)
                .and_then(|ty| module_export_call_return_type(ty, name))
                .as_ref()
                .and_then(lowered_checked_type)
            {
                return Some(kind);
            }
            if self.infer_checked_expr_type(base, known).is_some() {
                return self.infer_lowered_call_type(callee, args_vec);
            }
        }
        self.infer_lowered_call_type(callee, args_vec)
    }

    fn infer_lowered_binary_type(
        &self,
        op: BinaryOp,
        left: ExprId,
        right: ExprId,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<LoweredType> {
        match op {
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::And
            | BinaryOp::Or
            | BinaryOp::In
            | BinaryOp::NotIn => Some(LoweredType::Bool),
            BinaryOp::Add => {
                let left = self.infer_lowered_expr_type(left, known)?;
                let right = self.infer_lowered_expr_type(right, known)?;
                (left == right
                    && matches!(
                        left,
                        LoweredType::Int | LoweredType::Float | LoweredType::Str
                    ))
                .then_some(left)
            }
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                let left = self.infer_lowered_expr_type(left, known)?;
                let right = self.infer_lowered_expr_type(right, known)?;
                (left == right && matches!(left, LoweredType::Int | LoweredType::Float))
                    .then_some(left)
            }
            BinaryOp::ResultFallback => self
                .bodies
                .expr_types
                .get(&left)
                .and_then(lowered_result_fallback_type)
                .or_else(|| self.infer_lowered_expr_result_ok_type(left, known))
                .or_else(|| self.infer_lowered_expr_type(left, known)),
        }
    }

    fn infer_lowered_call_type(
        &self,
        callee: ExprId,
        args: &[ArenaCallArg],
    ) -> Option<LoweredType> {
        match self.program.arena.expr(callee).kind {
            ArenaExprKind::Ident(name) => {
                if name == "Path" && single_positional_arena_call_arg(args).is_some() {
                    return Some(LoweredType::Path);
                }
                if self.compact_tag_variant_arity(name).is_some() {
                    return Some(LoweredType::Tag);
                }
                if name == "range" {
                    return Some(LoweredType::List);
                }
                self.declarations
                    .pures
                    .get(&name)
                    .or_else(|| self.declarations.procs.get(&name))
                    .or_else(|| self.declarations.streams.get(&name))
                    .and_then(|sig| lowered_checked_type(&sig.return_ty))
            }
            ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name } => {
                if lowered_result_method_ok_type(name).is_some() {
                    return Some(LoweredType::Result);
                }
                if let Some(kind) = lowered_plain_method_type(name) {
                    return Some(kind);
                }
                let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind else {
                    return None;
                };
                if module == "process" && name == "command_argv" {
                    return Some(LoweredType::Command);
                }
                if let Some(kind) = lowered_module_callee_type(module, name) {
                    return Some(kind);
                }
                if module == "map" && name == "empty" {
                    return Some(LoweredType::Map);
                }
                if lowered_builtin_call_ok_type(module, name).is_some() {
                    return Some(LoweredType::Result);
                }
                if module == "bytes" && name == "concat" {
                    return Some(LoweredType::Bytes);
                }
                if let Some(sig) = self.compact_qualified_function_sig(module, name) {
                    return lowered_checked_type(&sig.return_ty);
                }
                let sig = self
                    .declarations
                    .pures
                    .get(&name)
                    .or_else(|| self.declarations.procs.get(&name));
                sig.and_then(|sig| lowered_checked_type(&sig.return_ty))
            }
            _ => None,
        }
    }

    fn infer_lowered_try_type(
        &self,
        expr: ExprId,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<LoweredType> {
        self.bodies
            .expr_types
            .get(&expr)
            .and_then(|ty| ty.result_ok())
            .filter(|ty| compact_checked_type_is_concrete(ty))
            .and_then(lowered_checked_type)
            .or_else(|| {
                self.infer_checked_expr_type(expr, known)
                    .as_ref()
                    .and_then(|ty| ty.result_ok())
                    .filter(|ty| compact_checked_type_is_concrete(ty))
                    .and_then(lowered_checked_type)
            })
            .or_else(|| match self.program.arena.expr(expr).kind {
                ArenaExprKind::Ident(name) => {
                    known.get(&name).and_then(|binding| binding.result_ok)
                }
                ArenaExprKind::Call { callee, args } => {
                    self.infer_lowered_call_ok_type(callee, args, known)
                }
                ArenaExprKind::Require { schema, .. } => {
                    lowered_arena_type(&self.program.arena, schema, self.declarations)
                }
                ArenaExprKind::Run(run) => lowered_arena_run_capture_type(&self.program.arena, run),
                ArenaExprKind::Spawn(_) => Some(LoweredType::ProcessHandle),
                ArenaExprKind::EnvGet { kind, .. } => match kind {
                    EnvGetKind::Str => Some(LoweredType::Str),
                    EnvGetKind::Path => Some(LoweredType::Path),
                    EnvGetKind::PathList => Some(LoweredType::List),
                },
                ArenaExprKind::EnvPathList => Some(LoweredType::List),
                _ => self.infer_lowered_expr_type(expr, known),
            })
    }

    fn infer_lowered_expr_result_ok_type(
        &self,
        value: ExprId,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<LoweredType> {
        self.bodies
            .expr_types
            .get(&value)
            .and_then(|ty| ty.result_ok())
            .and_then(lowered_checked_type)
            .or_else(|| match self.program.arena.expr(value).kind {
                ArenaExprKind::Ident(name) => {
                    known.get(&name).and_then(|binding| binding.result_ok)
                }
                ArenaExprKind::Call { callee, args } => {
                    self.infer_lowered_call_ok_type(callee, args, known)
                }
                _ => None,
            })
    }

    fn infer_lowered_call_ok_type(
        &self,
        callee: ExprId,
        args: crate::syntax::arena::ArenaRange,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<LoweredType> {
        match self.program.arena.expr(callee).kind {
            ArenaExprKind::Ident(name) => self
                .declarations
                .pures
                .get(&name)
                .or_else(|| self.declarations.procs.get(&name))
                .or_else(|| self.declarations.streams.get(&name))
                .and_then(|sig| sig.return_ty.result_ok())
                .and_then(lowered_checked_type),
            ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name } => {
                if name == "require" {
                    return Some(LoweredType::Module);
                }
                if name == "get"
                    && let Some(kind) = self.infer_checked_get_ok_type(base, args, known)
                {
                    return Some(kind);
                }
                if let Some(kind) = lowered_result_method_ok_type(name) {
                    return Some(kind);
                }
                if let Some(kind) = self
                    .infer_checked_expr_type(base, known)
                    .and_then(|ty| module_export_call_return_type(ty, name))
                    .as_ref()
                    .and_then(|ty| ty.result_ok())
                    .and_then(lowered_checked_type)
                {
                    return Some(kind);
                }
                if let Some(kind) = self.infer_lowered_builtin_call_ok_type(base, name) {
                    return Some(kind);
                }
                if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                    && let Some(kind) = lowered_module_callee_result_ok_type(module, name)
                {
                    return Some(kind);
                }
                if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                    && let Some(sig) = self.compact_qualified_function_sig(module, name)
                {
                    return sig.return_ty.result_ok().and_then(lowered_checked_type);
                }
                self.declarations
                    .pures
                    .get(&name)
                    .or_else(|| self.declarations.procs.get(&name))
                    .or_else(|| self.declarations.streams.get(&name))
                    .and_then(|sig| sig.return_ty.result_ok())
                    .and_then(lowered_checked_type)
            }
            _ => None,
        }
    }

    fn infer_checked_get_ok_type(
        &self,
        base: ExprId,
        args: crate::syntax::arena::ArenaRange,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<LoweredType> {
        let args = self.program.arena.call_args(args);
        let [arg] = args else {
            return None;
        };
        let base_ty = self.infer_checked_expr_type(base, known)?;
        match base_ty {
            Type::List(item) | Type::Map(item) => lowered_checked_type(&item),
            Type::Record(fields) => {
                let key = compact_call_arg_expr(arg)?;
                let ArenaExprKind::Str(key) = self.program.arena.expr(key).kind else {
                    return None;
                };
                let key = Name::intern(self.program.arena.string_literal(key).as_ref());
                fields.get(&key).and_then(lowered_checked_type)
            }
            _ => None,
        }
    }

    fn infer_lowered_builtin_call_ok_type(&self, base: ExprId, name: Name) -> Option<LoweredType> {
        if let Some(kind) = lowered_result_method_ok_type(name) {
            return Some(kind);
        }
        let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind else {
            return None;
        };
        if module == "regex" && name == "compile" {
            return Some(LoweredType::Regex);
        }
        if module == "fs" && name == "tempdir" {
            return Some(LoweredType::Record);
        }
        if module == "fs" && name == "root_path" {
            return Some(LoweredType::Path);
        }
        if module == "fs" && (name == "write" || name == "mkdir" || name == "remove") {
            return Some(LoweredType::Unit);
        }
        if module == "archive" && name == "tar_create" {
            return Some(LoweredType::Unit);
        }
        if module == "archive" && name == "tar_list" {
            return Some(LoweredType::Stream);
        }
        if module == "archive" && name == "tar_extract" {
            return Some(LoweredType::Unit);
        }
        if module == "json" && name == "encode" {
            return Some(LoweredType::Str);
        }
        if module == "json" && name == "decode" {
            return Some(LoweredType::Any);
        }
        None
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
        let mut lowered = Vec::with_capacity(ids.len());
        for stmt in prefix {
            lowered.push(self.lower_stmt_with_blocker_guard(
                *stmt,
                slots,
                current_function,
                item_slot,
            )?);
        }
        let tail = match self.program.arena.stmt(tail).kind {
            ArenaStmtKind::Expr(expr) => push_build_row!(
                self,
                stmt,
                BuildStmtRow::Return {
                    value: self.lower_expr(expr, slots, current_function, item_slot)?,
                }
            ),
            ArenaStmtKind::TailBareIdent(name) => push_build_row!(
                self,
                stmt,
                BuildStmtRow::Return {
                    value: self
                        .lower_bare_ident(name, slots)
                        .unwrap_or(push_build_row!(self, expr, BuildExprRow::Unit)),
                }
            ),
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
                            // Arms have multi-statement block bodies that produce a
                            // value; this can't be a `MatchExpr`. Lower it as a
                            // statement-match whose arm bodies return their tail.
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

    /// Lower a `retry` block body. Like `lower_block` it introduces a scope, but
    /// the trailing expression becomes a `BreakValue` so the block's value is
    /// carried out as `Break(Some(..))` rather than being discarded. Explicit
    /// `return` inside the body stays a `Return`, which the retry runtime treats
    /// as an escape (return from the enclosing proc), and `?` failures surface as
    /// `Propagate`, which the retry runtime treats as a retryable failure.
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
            let tail_stmt = match self.program.arena.stmt(tail).kind {
                ArenaStmtKind::Expr(expr) => push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::BreakValue {
                        value: self.lower_expr(expr, slots, current_function, item_slot)?,
                    }
                ),
                ArenaStmtKind::TailBareIdent(name) => push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::BreakValue {
                        value: self
                            .lower_bare_ident(name, slots)
                            .unwrap_or(push_build_row!(self, expr, BuildExprRow::Unit)),
                    }
                ),
                _ => {
                    self.lower_stmt_with_blocker_guard(tail, slots, current_function, item_slot)?
                }
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
        self.output.statements += 1;
        let lowered = match self.program.arena.stmt(id).kind {
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(value),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(value),
            } => {
                if let ArenaBindingTargetKind::Record { fields, .. } =
                    self.program.arena.binding_target(target).kind
                {
                    let source = self.lower_expr(value, slots, current_function, item_slot)?;
                    let field_list = self.program.arena.destructure_fields(fields).to_vec();
                    let mut lowered_fields = Vec::with_capacity(field_list.len());
                    for field in &field_list {
                        if slots.is_bound_non_capture(field.name) {
                            return None;
                        }
                        lowered_fields.push((field.name, slots.declare(field.name)));
                    }
                    return Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::LetRecord {
                            source,
                            fields: lowered_fields,
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
                if slots.is_bound_non_capture(name) {
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
                let binding_ty = self.lower_binding_checked_type(ty, value, slots);
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
                let slot = slots.declare_with_type(name, binding_ty);
                if let Some(value) = self.lower_int_expr_candidate(&value)
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
                if slots.is_bound_non_capture(name) {
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
                    .map(|ty| compact_runtime_type(&self.program.arena, ty, self.declarations))
                    .or_else(|| {
                        self.infer_lowered_run_binding_type(run)
                            .and_then(type_for_lowered_type)
                    });
                let slot = slots.declare_with_type(name, binding_ty);
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Let { slot, value }
                ))
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
                let value = match value {
                    ArenaExprOrRun::Expr(expr) => {
                        self.lower_expr(expr, slots, current_function, item_slot)?
                    }
                    ArenaExprOrRun::Run(run) => {
                        self.lower_run_binding_value(run, slots, current_function, item_slot)?
                    }
                };
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
                        } else if let Some(value) = self.lower_int_expr_candidate(&value)
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
                            span: self.program.arena.stmt(id).span,
                        }
                    )),
                    ArenaAssignTargetKind::Field { base, name }
                        if matches!(
                            self.program.arena.assign_target(base).kind,
                            ArenaAssignTargetKind::Name(_)
                        ) =>
                    {
                        if let Some(value) = self.lower_int_expr_candidate(&value)
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
                    ArenaAssignTargetKind::Index { base, index }
                        if matches!(
                            self.program.arena.assign_target(base).kind,
                            ArenaAssignTargetKind::Name(_)
                        ) =>
                    {
                        Some(push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::AssignIndex {
                                slot,
                                index: self.lower_expr(
                                    index,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                op,
                                value,
                                span: self.program.arena.stmt(id).span,
                            }
                        ))
                    }
                    _ => None,
                }
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                let branches = self.program.arena.if_branches(branches).to_vec();
                let mut lowered = Vec::with_capacity(branches.len());
                for branch in branches {
                    lowered.push((
                        self.lower_expr(branch.condition, slots, current_function, item_slot)?,
                        self.lower_block(branch.block, slots, current_function, item_slot)?,
                    ));
                }
                let else_body = match else_block {
                    Some(block) => {
                        Some(self.lower_block(block, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
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
                let condition = self.lower_expr(condition, slots, current_function, item_slot)?;
                let body = self.lower_block(block, slots, current_function, item_slot)?;
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
                if let ArenaBindingTargetKind::Record { fields, .. } =
                    self.program.arena.binding_target(target).kind
                {
                    if !self.program.arena.block(block).params.is_empty() {
                        return None;
                    }
                    let item_ty = self.infer_loop_item_checked_type(iter, slots);
                    let iter = self.lower_expr(iter, slots, current_function, item_slot)?;
                    let field_list = self.program.arena.destructure_fields(fields).to_vec();
                    let saved = slots.enter();
                    let mut lowered_fields = Vec::with_capacity(field_list.len());
                    for field in &field_list {
                        // Loop-scoped: may shadow an outer binding (restored on exit).
                        let field_ty = match &item_ty {
                            Some(Type::Record(fields)) => fields.get(&field.name).cloned(),
                            _ => None,
                        };
                        lowered_fields
                            .push((field.name, slots.declare_with_type(field.name, field_ty)));
                    }
                    let body =
                        self.lower_block_in_current_scope(block, slots, current_function, None)?;
                    slots.exit(saved);
                    return Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::ForRecord {
                            fields: lowered_fields,
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
                let text_or_iter = self.lower_expr(
                    str_lines_base.unwrap_or(iter),
                    slots,
                    current_function,
                    item_slot,
                )?;
                let item_ty = if let Some(base) = str_lines_base {
                    self.infer_checked_expr_type_with_slots(base, slots)
                        .or_else(|| self.infer_checked_expr_type(base, &self.top_level_known))
                        .and_then(|ty| match ty {
                            Type::Bytes => Some(Type::Bytes),
                            Type::Str => Some(Type::Str),
                            _ => None,
                        })
                        .or(Some(Type::Str))
                } else {
                    self.infer_loop_item_checked_type(iter, slots)
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
                    self.compact_match_scrutinee_result_types(value, slots);
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
            ArenaStmtKind::Guard {
                target,
                initializer,
                else_param,
                else_block,
                ..
            } => {
                let name = simple_binding_target(self.program, target)?;
                if is_discard_name(name) {
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
                let else_param_slot = else_param.map(|name| slots.declare(name));
                let else_body = self.lower_block_in_current_scope(
                    else_block,
                    slots,
                    current_function,
                    item_slot,
                );
                slots.exit(saved);
                let else_body = else_body?;
                let slot = slots.declare(name);
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Guard {
                        slot,
                        value,
                        else_param_slot,
                        else_body,
                        span: self.program.arena.stmt(id).span,
                    }
                ))
            }
            ArenaStmtKind::GuardedStmt {
                stmt,
                negate,
                condition,
            } => {
                let mut condition =
                    self.lower_expr(condition, slots, current_function, item_slot)?;
                if negate {
                    let false_value = push_build_row!(self, expr, BuildExprRow::Bool(false));
                    condition = push_build_row!(
                        self,
                        expr,
                        BuildExprRow::IfExpr {
                            branches: vec![(condition, false_value)],
                            else_value: push_build_row!(self, expr, BuildExprRow::Bool(true)),
                            span: self.program.arena.stmt(id).span,
                        }
                    );
                }
                let body = vec![self.lower_stmt_with_blocker_guard(
                    stmt,
                    slots,
                    current_function,
                    item_slot,
                )?];
                if let Some(condition) = self.lower_bool_expr_candidate(&condition) {
                    Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::IfBool {
                            branches: vec![(condition, body)],
                            else_body: None,
                        }
                    ))
                } else {
                    Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::If {
                            branches: vec![(condition, body)],
                            else_body: None,
                        }
                    ))
                }
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
            ArenaStmtKind::Expr(value) => Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Expr {
                    value: self.lower_expr(value, slots, current_function, item_slot)?,
                    span: self.program.arena.stmt(id).span,
                }
            )),
            ArenaStmtKind::Defer(ArenaExprOrRun::Expr(value)) => {
                let value = self.lower_expr(value, slots, current_function, item_slot)?;
                Some(push_build_row!(self, stmt, BuildStmtRow::Defer { value }))
            }
            ArenaStmtKind::Defer(ArenaExprOrRun::Run(run)) => {
                let value =
                    self.lower_run_binding_value(run, slots, current_function, item_slot)?;
                Some(push_build_row!(self, stmt, BuildStmtRow::Defer { value }))
            }
            ArenaStmtKind::TailBareIdent(name) => {
                let value = self.lower_bare_ident(name, slots)?;
                Some(push_build_row!(self, stmt, BuildStmtRow::Return { value }))
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
                    if let Some(value) =
                        lower_command_word_reference(text, slots, span, &self.scratch)
                    {
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
            ArenaCommandArgKind::SpliceName(name) => self.lower_bare_ident(*name, slots),
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
                let run = LoweredSpawnRun {
                    target,
                    args: lowered_args,
                    env: lowered_env,
                    redirections,
                    timeout,
                    cpu_max,
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
                propagate: propagate_internally,
                assert_success,
                span: self.program.arena.span(run.span),
            };
            let capture = push_build_row!(self, expr, BuildExprRow::RunCapture(Box::new(capture)));
            if run.propagate && capture_kind {
                Some(push_build_row!(self, expr, BuildExprRow::Try(capture)))
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
                lowered_segments.push(LoweredRunPipelineSegment {
                    kind: segment.kind,
                    target,
                    args: lowered_args,
                    env: lowered_env,
                    redirections,
                    timeout,
                    cpu_max,
                });
            }
            let pipeline = push_build_row!(
                self,
                expr,
                BuildExprRow::RunPipeline {
                    segments: lowered_segments,
                    propagate: run.propagate,
                    span: self.program.arena.span(run.span),
                }
            );
            if run.propagate {
                Some(push_build_row!(self, expr, BuildExprRow::Try(pipeline)))
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
                            BuildExprRow::FmtString(lowered)
                        ))
                    }
                }
                ArenaCommandArgKind::SpliceName(name) => {
                    LoweredRunArgKind::Splice(self.lower_bare_ident(*name, slots)?)
                }
                ArenaCommandArgKind::SpliceExpr(expr) => LoweredRunArgKind::Splice(
                    self.lower_expr(*expr, slots, current_function, item_slot)?,
                ),
            };
        Some(LoweredRunArg { kind, span })
    }

    fn lower_expr(
        &mut self,
        id: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        self.output.expressions += 1;
        let span = self.program.arena.expr(id).span;
        let lowered = match self.program.arena.expr(id).kind {
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
            ArenaExprKind::Record(fields) => {
                self.lower_record(fields, slots, current_function, item_slot)
            }
            ArenaExprKind::List(items) => {
                let items = self.program.arena.expr_ids(items).collect::<Vec<_>>();
                let mut lowered = Vec::with_capacity(items.len());
                for item in items {
                    lowered.push(self.lower_expr(item, slots, current_function, item_slot)?);
                }
                Some(push_build_row!(self, expr, BuildExprRow::List(lowered)))
            }
            ArenaExprKind::ListComp {
                expr,
                target,
                iter,
                condition,
            } => {
                let iter = self.lower_expr(iter, slots, current_function, item_slot)?;
                let saved = slots.enter();
                let target = self.lower_comp_target(target, slots)?;
                let condition = match condition {
                    Some(condition) => {
                        Some(self.lower_expr(condition, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                let value = self.lower_expr(expr, slots, current_function, item_slot)?;
                slots.exit(saved);
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ListComp {
                        value,
                        target: Box::new(target),
                        iter,
                        condition,
                        span,
                    }
                ))
            }
            ArenaExprKind::MapComp {
                key,
                value,
                target,
                iter,
                condition,
            } => {
                let iter = self.lower_expr(iter, slots, current_function, item_slot)?;
                let saved = slots.enter();
                let target = self.lower_comp_target(target, slots)?;
                let condition = match condition {
                    Some(condition) => {
                        Some(self.lower_expr(condition, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                let key = self.lower_expr(key, slots, current_function, item_slot)?;
                let value = self.lower_expr(value, slots, current_function, item_slot)?;
                slots.exit(saved);
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::MapComp {
                        key,
                        value,
                        target: Box::new(target),
                        iter,
                        condition,
                        span,
                    }
                ))
            }
            ArenaExprKind::Pipeline { input, stages } => {
                let pipe_stages = self.program.arena.pipe_stages(stages).to_vec();
                let mut lowered_stages = Vec::with_capacity(pipe_stages.len());
                let mut current_ty = self
                    .infer_checked_expr_type(input, &self.top_level_known)
                    .or_else(|| self.infer_checked_expr_type_with_slots(input, slots))
                    .map(|ty| ty.result_ok().cloned().unwrap_or(ty));
                for pipe_stage in &pipe_stages {
                    let ArenaPipeStageKind::Stream(ref stream_stage) = pipe_stage.kind else {
                        lowered_stages.push(LoweredPipelineStage::Collect);
                        continue;
                    };
                    let item_ty = current_ty.as_ref().and_then(stream_item_type);
                    lowered_stages.push(self.lower_pipeline_stage(
                        stream_stage,
                        slots,
                        current_function,
                        item_ty,
                    )?);
                    current_ty = current_ty.and_then(|ty| {
                        self.infer_checked_stream_stage_type_with_slots(&ty, stream_stage, slots)
                    });
                }
                self.fuse_par_map_flat_map_reduce_by(&mut lowered_stages);
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ListPipeline {
                        input: self.lower_expr(input, slots, current_function, item_slot)?,
                        stages: lowered_stages,
                        span,
                    }
                ))
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                let stages = self.program.arena.stream_stages(stages).to_vec();
                let mut lowered_stages = Vec::with_capacity(stages.len());
                let mut current_ty = self
                    .infer_checked_expr_type(input, &self.top_level_known)
                    .or_else(|| self.infer_checked_expr_type_with_slots(input, slots))
                    .map(|ty| ty.result_ok().cloned().unwrap_or(ty));
                for stage in &stages {
                    let item_ty = current_ty.as_ref().and_then(stream_item_type);
                    lowered_stages.push(self.lower_pipeline_stage(
                        stage,
                        slots,
                        current_function,
                        item_ty,
                    )?);
                    current_ty = current_ty.and_then(|ty| {
                        self.infer_checked_stream_stage_type_with_slots(&ty, stage, slots)
                    });
                }
                self.fuse_par_map_flat_map_reduce_by(&mut lowered_stages);
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ListPipeline {
                        input: self.lower_expr(input, slots, current_function, item_slot)?,
                        stages: lowered_stages,
                        span,
                    }
                ))
            }
            ArenaExprKind::Require { value, schema } => {
                lowered_arena_type(&self.program.arena, schema, self.declarations)?;
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Require {
                        value: self.lower_expr(value, slots, current_function, item_slot)?,
                        check: LoweredTypeCheck {
                            ty: compact_runtime_type(
                                &self.program.arena,
                                schema,
                                self.declarations
                            ),
                            name: compact_type_expr_name(&self.program.arena, schema),
                        },
                        span,
                    }
                ))
            }
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
            ArenaExprKind::Binary { op, left, right } if lowered_binary_op(op) => {
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Binary {
                        op,
                        left: self.lower_expr(left, slots, current_function, item_slot)?,
                        right: self.lower_expr(right, slots, current_function, item_slot)?,
                        span,
                    }
                ))
            }
            ArenaExprKind::Binary {
                op: BinaryOp::ResultFallback,
                left,
                right,
            } => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ResultFallback {
                    left: self.lower_expr(left, slots, current_function, item_slot)?,
                    right: self.lower_expr(right, slots, current_function, item_slot)?,
                }
            )),
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                let branches = self.program.arena.if_expr_branches(branches).to_vec();
                let mut lowered = Vec::with_capacity(branches.len());
                for branch in branches {
                    lowered.push((
                        self.lower_expr(branch.condition, slots, current_function, item_slot)?,
                        self.lower_expr(branch.value, slots, current_function, item_slot)?,
                    ));
                }
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::IfExpr {
                        branches: lowered,
                        else_value: self.lower_expr(
                            else_value,
                            slots,
                            current_function,
                            item_slot,
                        )?,
                        span,
                    }
                ))
            }
            ArenaExprKind::Match { value, arms } => {
                if let Some(expr) =
                    self.lower_str_match_expr(value, arms, span, slots, current_function, item_slot)
                {
                    return Some(expr);
                }
                if let Some(expr) =
                    self.lower_tag_match_expr(value, arms, span, slots, current_function, item_slot)
                {
                    return Some(expr);
                }
                let arms = self.program.arena.match_expr_arms(arms).to_vec();
                let (ok_binding_ty, err_binding_ty) =
                    self.compact_match_scrutinee_result_types(value, slots);
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
            ArenaExprKind::NullSafeField { base, name } => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Field {
                    base: push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Try(self.lower_expr(
                            base,
                            slots,
                            current_function,
                            item_slot,
                        )?)
                    ),
                    name: name.as_str(),
                    span,
                }
            )),
            ArenaExprKind::Index { base, index } => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Index {
                    base: self.lower_expr(base, slots, current_function, item_slot)?,
                    index: self.lower_expr(index, slots, current_function, item_slot)?,
                    span,
                }
            )),
            ArenaExprKind::Slice { base, start, end } => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Slice {
                    base: self.lower_expr(base, slots, current_function, item_slot)?,
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
                let expr_span = self.program.arena.expr(expr).span;
                if let ArenaExprKind::Call { callee, args } = self.program.arena.expr(expr).kind {
                    let args_vec = self.program.arena.call_args(args).to_vec();
                    if let ArenaExprKind::Field { base, name } =
                        self.program.arena.expr(callee).kind
                        && let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                    {
                        if module == "fs" && name == "files" {
                            let options =
                                lower_fs_files_args(&self.program.arena, &args_vec, true)?;
                            return Some(push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try(push_build_row!(
                                    self,
                                    expr,
                                    BuildExprRow::FsFiles {
                                        root: self.lower_expr(
                                            options.root,
                                            slots,
                                            current_function,
                                            item_slot,
                                        )?,
                                        gitignore: options.gitignore,
                                        stat: options.stat,
                                        hidden: options.hidden,
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
                                ))
                            ));
                        }
                        if module == "fs" && name == "walk" {
                            let options =
                                lower_fs_files_args(&self.program.arena, &args_vec, false)?;
                            return Some(push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try(push_build_row!(
                                    self,
                                    expr,
                                    BuildExprRow::FsWalk {
                                        root: self.lower_expr(
                                            options.root,
                                            slots,
                                            current_function,
                                            item_slot,
                                        )?,
                                        gitignore: options.gitignore,
                                        stat: options.stat,
                                        hidden: options.hidden,
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
                                ))
                            ));
                        }
                    }
                }
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Try(self.lower_expr(expr, slots, current_function, item_slot,)?)
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
            ArenaExprKind::Loop { block } => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Loop {
                    body: self.lower_block(block, slots, current_function, item_slot)?,
                    span,
                }
            )),
            ArenaExprKind::Retry { delays, block } => {
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
                        body: self.lower_retry_block(block, slots, current_function, item_slot)?,
                        span,
                    }
                ))
            }
            ArenaExprKind::EnvGet { kind, name } => {
                let op = match kind {
                    EnvGetKind::Path => RuntimeOp::EnvPath,
                    _ => RuntimeOp::EnvGet,
                };
                let name = push_build_row!(self, expr, BuildExprRow::Str(name.to_string().into()));
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ModuleCall {
                        op,
                        args: vec![name],
                        span,
                    }
                ))
            }
            ArenaExprKind::EnvPathList => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ModuleCall {
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
                Some(lowered)
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

    fn lower_env_field_method_call(
        &mut self,
        base: crate::syntax::arena::ExprId,
        _method: crate::symbol::Name,
        args_vec: &[ArenaCallArg],
        span: crate::source::Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let base_kind = self.program.arena.expr(base).kind;
        let (field_name, method_name) = match base_kind {
            ArenaExprKind::Ident(module) if module == "env" => return None,
            ArenaExprKind::Field {
                base: inner,
                name: method_name,
            } => {
                if !is_env_module_expr(&self.program.arena, inner) {
                    return None;
                }
                let inner_kind = self.program.arena.expr(inner).kind;
                let field_name = match inner_kind {
                    ArenaExprKind::Field { name, .. } => name,
                    _ => return None,
                };
                (field_name, method_name)
            }
            _ => return None,
        };
        let positional = positional_call_args(args_vec)?;
        let mut lowered_args = Vec::with_capacity(positional.len());
        for arg in &positional {
            lowered_args.push(self.lower_expr(*arg, slots, current_function, item_slot)?);
        }
        let op = match (field_name.as_str().as_str(), method_name.as_str().as_str()) {
            ("PATH", "prepend") => RuntimeOp::EnvPathPrepend,
            ("PATH", "append") => RuntimeOp::EnvPathAppend,
            ("PATH", "pop") => RuntimeOp::EnvPathPop,
            _ => return None,
        };
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::ModuleCall {
                op,
                args: lowered_args,
                span,
            }
        ))
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
                        op: RuntimeOp::EnvGet,
                        args: vec![name],
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
                                op,
                                args: vec![arg],
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

    fn lower_record(
        &mut self,
        fields: crate::syntax::arena::ArenaRange,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let fields = self.program.arena.record_fields(fields).to_vec();
        let mut lowered = Vec::with_capacity(fields.len());
        for field in fields {
            match field.kind {
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
                ArenaCallArgKind::Named { .. } => return None,
            }
        }
        Some(lowered)
    }

    fn lower_function_call_args(
        &mut self,
        args: &[ArenaCallArg],
        params: Option<&[CallableParamType]>,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<LoweredCallArg>> {
        if let Some(params) = params
            && let Some(exprs) = compact_named_function_call_arg_exprs(args, params)
        {
            return exprs
                .into_iter()
                .map(|expr| {
                    self.lower_expr(expr, slots, current_function, item_slot)
                        .map(LoweredCallArg::Single)
                })
                .collect();
        }
        self.lower_call_args(args, slots, current_function, item_slot)
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
        if let Some(error) =
            self.lower_compact_error_expr(callee, &args_vec, slots, current_function, item_slot)
        {
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Error(Box::new(error))
            ));
        }
        match self.program.arena.expr(callee).kind {
            ArenaExprKind::Field { base, name } => {
                if let Some(env_call) = self.lower_env_field_method_call(
                    callee,
                    name,
                    &args_vec,
                    span,
                    slots,
                    current_function,
                    item_slot,
                ) {
                    return Some(env_call);
                }
                if name == "call" {
                    let args =
                        self.lower_call_args(&args_vec, slots, current_function, item_slot)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::DynamicCall {
                            callee: self.lower_expr(base, slots, current_function, item_slot,)?,
                            args,
                            span,
                        }
                    ));
                }
                if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind {
                    if module.as_str() == "error" && name.as_str() == "fail" {
                        let [ArenaCallArg {
                            kind: ArenaCallArgKind::Positional(message),
                        }] = args_vec.as_slice()
                        else {
                            return None;
                        };
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::Fail {
                                message: self.lower_expr(
                                    *message,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                    if module == "Path" && name == "parse_bytes" {
                        let positional = positional_call_args(&args_vec)?;
                        if positional.len() == 1 {
                            return Some(push_build_row!(
                                self,
                                expr,
                                BuildExprRow::ModuleCall {
                                    op: RuntimeOp::PathParseBytes,
                                    args: self.lower_expr_ids(
                                        &positional,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?,
                                    span,
                                }
                            ));
                        }
                    }
                    if module == "fs" && (name == "ls" || name == "children") {
                        let options = lower_fs_list_args(&args_vec)?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::FsList {
                                op: if name == "ls" {
                                    RuntimeOp::FsLs
                                } else {
                                    RuntimeOp::FsChildren
                                },
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
                            lower_fs_files_args(&self.program.arena, &args_vec, true)?;
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
                                gitignore: options.gitignore,
                                stat: options.stat,
                                hidden: options.hidden,
                                exts: match options.exts {
                                    Some(exts) => Some(self.lower_expr(
                                        exts,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?),
                                    None => None,
                                },
                                result_wrapped: false,
                                span,
                            }
                        ));
                    }
                    if module == "fs" && name == "walk" {
                        let options =
                            lower_fs_files_args(&self.program.arena, &args_vec, false)?;
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
                                gitignore: options.gitignore,
                                stat: options.stat,
                                hidden: options.hidden,
                                exts: match options.exts {
                                    Some(exts) => Some(self.lower_expr(
                                        exts,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?),
                                    None => None,
                                },
                                result_wrapped: false,
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
                    if module == "fs" && name == "write" {
                        let options = lower_fs_write_args(&args_vec)?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::FsWrite {
                                path: self.lower_expr(
                                    options.path,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                data: self.lower_expr(
                                    options.data,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                    if module == "fs" && name == "mkdir" {
                        let options = lower_fs_mkdir_args(&args_vec)?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::FsMkdir {
                                path: self.lower_expr(
                                    options.path,
                                    slots,
                                    current_function,
                                    item_slot,
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
                    if module == "fs" && name == "remove" {
                        let options = lower_fs_remove_args(&args_vec)?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::FsRemove {
                                path: self.lower_expr(
                                    options.path,
                                    slots,
                                    current_function,
                                    item_slot,
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
                    if module == "archive" && name == "tar_create" {
                        let options = lower_archive_tar_create_args(&args_vec)?;
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
                        let options = lower_process_command_argv_args(&args_vec)?;
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
                            span,
                        };
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ProcessCommandArgv(Box::new(command))
                        ));
                    }
                    if let Some(module_call) = lowered_module_call_args(module, name, &args_vec) {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                op: module_call.op,
                                args: self.lower_expr_ids(
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
                if name == "write" || name == "write_atomic" {
                    let options = lower_path_write_args(&args_vec)?;
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
                if name == "contains" && args_vec.len() == 1 {
                    let ArenaCallArgKind::Positional(needle) = args_vec[0].kind else {
                        return None;
                    };
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Contains {
                            receiver: self.lower_expr(base, slots, current_function, item_slot,)?,
                            needle: self.lower_expr(needle, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "mkdir" {
                    let options = lower_path_mkdir_args(&args_vec)?;
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
                    && let Some(options) = lower_path_remove_args(&args_vec)
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
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::HashVerifyFile {
                            path: self.lower_expr(
                                options.path,
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            algorithm: options.algorithm,
                            expected: self.lower_expr(
                                options.expected,
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            span,
                        }
                    ));
                }
                if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                    && let Some(module_call) = lowered_module_call_args(module, name, &args_vec)
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            op: module_call.op,
                            args: self.lower_expr_ids(
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
                                op: RuntimeOp::PathParseBytes,
                                args: self.lower_expr_ids(
                                    positional,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                    if module == "fs" && name == "close_root" && positional.len() == 1 {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::FsCloseRoot {
                                root: self.lower_expr(
                                    positional[0],
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                    if module == "fs" && name == "root_path" && positional.len() == 1 {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::FsRootPath {
                                root: self.lower_expr(
                                    positional[0],
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
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
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::HashVerifyFile {
                                path: self.lower_expr(
                                    options.path,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                algorithm: options.algorithm,
                                expected: self.lower_expr(
                                    options.expected,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                    if let Some(module_call) = lowered_module_call_args(module, name, &args_vec) {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                op: module_call.op,
                                args: self.lower_expr_ids(
                                    &module_call.args,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                    let qualified = QualifiedName::new(module, name);
                    if self.compact_qualified_function_available(qualified) {
                        let params = self
                            .compact_qualified_function_sig(module, name)
                            .map(|sig| sig.params.clone());
                        let args = self
                            .lower_function_call_args(
                                &args_vec,
                                params.as_deref(),
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
                        && let Some(call_args) = lowered_module_call_args(module, name, &args_vec)
                    {
                        let mut lowered = Vec::with_capacity(call_args.args.len());
                        for arg in call_args.args {
                            lowered.push(self.lower_expr(
                                arg,
                                slots,
                                current_function,
                                item_slot,
                            )?);
                        }
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
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
                // Recognize `<text>.starts_with(n)` / `.ends_with(n)` and lower to the
                // direct StrPredicate node (the bool-condition path then specializes it
                // into StrPredicateSlot/TrimStrPredicateSlot for slot/trim receivers).
                let str_predicate = match name.as_str().as_str() {
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
                let method_args = lowered_method_call_args(name, &args_vec)?;
                if !self.lowered_method_supported_for_receiver(base, name, method_args.len(), slots)
                {
                    return None;
                }
                let receiver = self.lower_expr(base, slots, current_function, item_slot)?;
                let mut lowered_args = Vec::with_capacity(method_args.len());
                for arg in method_args {
                    lowered_args.push(self.lower_expr(arg, slots, current_function, item_slot)?);
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
                                    default: args.next(),
                                    span,
                                }
                            )
                        }
                        _ => unreachable!(),
                    });
                }
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Method {
                        receiver,
                        name: name.as_str(),
                        args: lowered_args,
                        span,
                    }
                ))
            }
            ArenaExprKind::NullSafeField { base, name } => {
                if name == "read_text" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathReadText {
                            path: push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try(self.lower_expr(
                                    base,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?)
                            ),
                            span,
                        }
                    ));
                }
                if name == "read_bytes" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathReadBytes {
                            path: push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try(self.lower_expr(
                                    base,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?)
                            ),
                            span,
                        }
                    ));
                }
                if name == "exists" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathExists {
                            path: push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try(self.lower_expr(
                                    base,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?)
                            ),
                            span,
                        }
                    ));
                }
                if name == "executable" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathExecutable {
                            path: push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try(self.lower_expr(
                                    base,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?)
                            ),
                            span,
                        }
                    ));
                }
                if name == "du" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathDu {
                            path: push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try(self.lower_expr(
                                    base,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?)
                            ),
                            span,
                        }
                    ));
                }
                if name == "metadata" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathMetadata {
                            path: push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try(self.lower_expr(
                                    base,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?)
                            ),
                            span,
                        }
                    ));
                }
                if name == "readlink" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathReadlink {
                            path: push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try(self.lower_expr(
                                    base,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?)
                            ),
                            span,
                        }
                    ));
                }
                if name == "resolve" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathResolve {
                            path: push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try(self.lower_expr(
                                    base,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?)
                            ),
                            span,
                        }
                    ));
                }
                if name == "write" || name == "write_atomic" {
                    let options = lower_path_write_args(&args_vec)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathWrite {
                            path: push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try(self.lower_expr(
                                    base,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?)
                            ),
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
                    let options = lower_path_mkdir_args(&args_vec)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathMkdir {
                            path: push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try(self.lower_expr(
                                    base,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?)
                            ),
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
                    && let Some(options) = lower_path_remove_args(&args_vec)
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathRemove {
                            path: push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try(self.lower_expr(
                                    base,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?)
                            ),
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
                let method_args = lowered_method_call_args(name, &args_vec)?;
                if !lowered_method_name(&name.as_str()) {
                    if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                        && let Some(call_args) = lowered_module_call_args(module, name, &args_vec)
                    {
                        let mut lowered = Vec::with_capacity(call_args.args.len());
                        for arg in call_args.args {
                            lowered.push(self.lower_expr(
                                arg,
                                slots,
                                current_function,
                                item_slot,
                            )?);
                        }
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
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
                let mut lowered_args = Vec::with_capacity(method_args.len());
                for arg in method_args {
                    lowered_args.push(self.lower_expr(arg, slots, current_function, item_slot)?);
                }
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Method {
                        receiver: push_build_row!(
                            self,
                            expr,
                            BuildExprRow::Try(self.lower_expr(
                                base,
                                slots,
                                current_function,
                                item_slot,
                            )?)
                        ),
                        name: name.as_str(),
                        args: lowered_args,
                        span,
                    }
                ))
            }
            ArenaExprKind::Ident(name) => {
                if name == "abort" {
                    let options = lower_abort_args(&args_vec)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Abort {
                            status: self.lower_expr(
                                options.status,
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            force: match options.force {
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
                let positional = positional_call_args(&args_vec);
                if let Some(arity) = self.compact_tag_variant_arity(name) {
                    let positional = positional.as_ref()?;
                    if positional.len() != arity {
                        return None;
                    }
                    let mut fields = Vec::with_capacity(positional.len());
                    for arg in positional {
                        fields.push(self.lower_expr(*arg, slots, current_function, item_slot)?);
                    }
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Tag {
                            name: Arc::<str>::from(name.as_str().as_str()),
                            fields,
                        }
                    ));
                }
                if (name == "Ok" || name == "Err")
                    && let Some(positional) = positional.as_ref()
                {
                    let value = match positional.as_slice() {
                        [] if name == "Ok" => push_build_row!(self, expr, BuildExprRow::Unit),
                        [value] => self.lower_expr(*value, slots, current_function, item_slot)?,
                        _ => return None,
                    };
                    return if name == "Ok" {
                        Some(push_build_row!(self, expr, BuildExprRow::Ok(value)))
                    } else {
                        Some(push_build_row!(self, expr, BuildExprRow::Err(value)))
                    };
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
                            op: RuntimeOp::EnvGet,
                            args: vec![self.lower_expr(
                                *value,
                                slots,
                                current_function,
                                item_slot
                            )?],
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
                    slots,
                    current_function,
                    item_slot,
                )?;
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
        callee: ExprId,
        args: &[ArenaCallArg],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<LoweredErrorExpr> {
        if let ArenaExprKind::Ident(name) = self.program.arena.expr(callee).kind
            && name == Name::ERROR
        {
            let mut kind = None;
            let mut message = None;
            for (index, arg) in args.iter().enumerate() {
                let (name, value) = match arg.kind {
                    ArenaCallArgKind::Named { name, value, .. } => (Some(name), value),
                    ArenaCallArgKind::Positional(value) => (None, value),
                    ArenaCallArgKind::Splice { .. } => return None,
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
            return Some(LoweredErrorExpr::Simple {
                kind: kind.unwrap_or_default(),
                message: message.unwrap_or_default(),
            });
        }

        let ArenaExprKind::Field {
            base,
            name: variant,
        } = self.program.arena.expr(callee).kind
        else {
            return None;
        };
        let family_key = compact_error_family_key(self.program, base)?;
        let family_name = compact_error_family_display(family_key);
        let info = compact_error_family_info(self.declarations, family_key)
            .and_then(|family| family.variants.get(&variant))?;
        let expected_fields = info.fields.keys().copied().collect::<Vec<_>>();
        let mut fields = Vec::with_capacity(expected_fields.len());
        let mut seen = FxHashSet::default();
        let mut positional_index = 0usize;
        for arg in args {
            let (field, value) = match arg.kind {
                ArenaCallArgKind::Named { name, value, .. } => {
                    if !info.fields.contains_key(&name) || !seen.insert(name) {
                        return None;
                    }
                    (name, value)
                }
                ArenaCallArgKind::Positional(value) => {
                    let field = *expected_fields.get(positional_index)?;
                    positional_index += 1;
                    if !seen.insert(field) {
                        return None;
                    }
                    (field, value)
                }
                ArenaCallArgKind::Splice { .. } => return None,
            };
            fields.push((
                Arc::<str>::from(field.as_str().as_str()),
                self.lower_expr(value, slots, current_function, item_slot)?,
            ));
        }
        if expected_fields.iter().any(|field| !seen.contains(field)) {
            return None;
        }
        Some(LoweredErrorExpr::Structured {
            family: family_name,
            variant: variant.to_string(),
            fields,
            facets: info.facets.clone(),
        })
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
        let Some(function) = compact_function_defs(self.program)
            .into_iter()
            .find(|function| function.key == key)
        else {
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
        let qualified = QualifiedName::new(module, name);
        self.declarations
            .qualified_pures
            .get(&qualified)
            .or_else(|| self.declarations.qualified_procs.get(&qualified))
            .or_else(|| self.declarations.qualified_streams.get(&qualified))
    }

    fn lower_bare_ident(&self, name: Name, slots: &SlotScope) -> Option<BuildExprId> {
        slots
            .resolve(name)
            .map(|slot| push_build_row!(self, expr, BuildExprRow::Param(slot)))
            .or_else(|| {
                if let Some(key) = self.compact_unqualified_function_key(name) {
                    let pure = self
                        .functions
                        .is_none_or(|functions| functions.pure_contains(key));
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
                (self.compact_tag_variant_arity(name) == Some(0)).then(|| {
                    push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Tag {
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
        match stage.kind {
            StreamStageKind::TextStreamLines => {
                if !stage.options.is_empty() {
                    return None;
                }
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::TextLines)
            }
            StreamStageKind::JsonLines | StreamStageKind::JsonStream => {
                if !stage.options.is_empty() {
                    return None;
                }
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::JsonLines)
            }
            StreamStageKind::Enumerate => {
                if !stage.options.is_empty() {
                    return None;
                }
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Enumerate)
            }
            StreamStageKind::Zip => {
                if !stage.options.is_empty() || stage.block.is_some() {
                    return None;
                }
                let [arg] = self.program.arena.call_args(stage.args) else {
                    return None;
                };
                let ArenaCallArgKind::Positional(other) = arg.kind else {
                    return None;
                };
                Some(LoweredPipelineStage::Zip {
                    other: self.lower_expr(other, slots, current_function, None)?,
                })
            }
            StreamStageKind::Sort => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Sort {
                    descending: self.lower_pipeline_stage_desc_option(
                        stage,
                        slots,
                        current_function,
                    )?,
                })
            }
            StreamStageKind::Sum => {
                if !stage.options.is_empty() {
                    return None;
                }
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Sum)
            }
            StreamStageKind::Collect => {
                if !stage.options.is_empty() {
                    return None;
                }
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Collect)
            }
            StreamStageKind::First => {
                if !stage.options.is_empty() {
                    return None;
                }
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::First)
            }
            StreamStageKind::Last => {
                if !stage.options.is_empty() {
                    return None;
                }
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Last)
            }
            StreamStageKind::Min => {
                if !stage.options.is_empty() {
                    return None;
                }
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Min)
            }
            StreamStageKind::Max => {
                if !stage.options.is_empty() {
                    return None;
                }
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Max)
            }
            StreamStageKind::SortBy => {
                if let Some((slot, key)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::SortBy {
                        slot,
                        key,
                        descending: self.lower_pipeline_stage_desc_option(
                            stage,
                            slots,
                            current_function,
                        )?,
                    });
                }
                let (slot, key) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::SortBy {
                    slot,
                    key,
                    descending: self.lower_pipeline_stage_desc_option(
                        stage,
                        slots,
                        current_function,
                    )?,
                })
            }
            StreamStageKind::UniqueBy => {
                if !stage.options.is_empty() {
                    return None;
                }
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
                // `--jobs=N` is a parallelism hint; serial execution produces
                // identical (encounter-ordered) results, so accept and ignore it.
                for option in self.program.arena.stream_options(stage.options) {
                    if option.name.as_str() != "jobs" {
                        return None;
                    }
                }
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
                // `--jobs=N` is a parallelism hint; serial execution produces
                // identical results, so accept and ignore it.
                for option in self.program.arena.stream_options(stage.options) {
                    if option.name.as_str() != "jobs" {
                        return None;
                    }
                }
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
                if !stage.options.is_empty() {
                    return None;
                }
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
                if !stage.options.is_empty() {
                    return None;
                }
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
                if !stage.options.is_empty() {
                    return None;
                }
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
                if !stage.options.is_empty() {
                    return None;
                }
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
                if !stage.options.is_empty() {
                    return None;
                }
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
            StreamStageKind::Take | StreamStageKind::Drop => {
                if !stage.options.is_empty() {
                    return None;
                }
                if stage.block.is_some() {
                    return None;
                }
                let [arg] = self.program.arena.call_args(stage.args) else {
                    return None;
                };
                let ArenaCallArgKind::Positional(arg) = arg.kind else {
                    return None;
                };
                let arg = self.lower_expr(arg, slots, current_function, None)?;
                match stage.kind {
                    StreamStageKind::Take => Some(LoweredPipelineStage::Take(arg)),
                    StreamStageKind::Drop => Some(LoweredPipelineStage::Drop(arg)),
                    _ => None,
                }
            }
            StreamStageKind::Repeat => {
                if !stage.options.is_empty() {
                    return None;
                }
                if stage.block.is_some() {
                    return None;
                }
                let [arg] = self.program.arena.call_args(stage.args) else {
                    return None;
                };
                let ArenaCallArgKind::Positional(arg) = arg.kind else {
                    return None;
                };
                let count = self.lower_expr(arg, slots, current_function, None)?;
                Some(LoweredPipelineStage::Repeat { count })
            }
            StreamStageKind::Range => {
                if !stage.options.is_empty() {
                    return None;
                }
                if stage.block.is_some() {
                    return None;
                }
                let [start_arg, end_arg] = self.program.arena.call_args(stage.args) else {
                    return None;
                };
                let ArenaCallArgKind::Positional(start) = start_arg.kind else {
                    return None;
                };
                let ArenaCallArgKind::Positional(end) = end_arg.kind else {
                    return None;
                };
                let start = self.lower_expr(start, slots, current_function, None)?;
                let end = self.lower_expr(end, slots, current_function, None)?;
                Some(LoweredPipelineStage::Range { start, end })
            }
            StreamStageKind::BytesChunks => {
                if !stage.options.is_empty() || stage.block.is_some() {
                    return None;
                }
                let [arg] = self.program.arena.call_args(stage.args) else {
                    return None;
                };
                let ArenaCallArgKind::Positional(arg) = arg.kind else {
                    return None;
                };
                Some(LoweredPipelineStage::BytesChunks {
                    size: self.lower_expr(arg, slots, current_function, None)?,
                })
            }
            StreamStageKind::Batch => {
                if stage.block.is_some() {
                    return None;
                }
                let options = self.program.arena.stream_options(stage.options);
                match options {
                    [] => None,
                    [option] if option.name == "count" => {
                        let value = option.value?;
                        let count = self.lower_expr(value, slots, current_function, None)?;
                        Some(LoweredPipelineStage::BatchCount { count })
                    }
                    [option] if option.name == "max-argv" => {
                        let max_argv = match option.value {
                            Some(value) => {
                                Some(self.lower_expr(value, slots, current_function, None)?)
                            }
                            None => None,
                        };
                        Some(LoweredPipelineStage::BatchMaxArgv { max_argv })
                    }
                    [option] if option.name == "max-bytes" => {
                        let value = option.value?;
                        let max_bytes = self.lower_expr(value, slots, current_function, None)?;
                        Some(LoweredPipelineStage::BatchMaxBytes { max_bytes })
                    }
                    _ => None,
                }
            }
            StreamStageKind::ParMap => {
                if !stage.args.is_empty() {
                    return None;
                }
                let mut jobs = None;
                for option in self.program.arena.stream_options(stage.options) {
                    if option.name.as_str() != "jobs" {
                        return None;
                    }
                    let value = option.value?;
                    jobs = Some(self.lower_expr(value, slots, current_function, None)?);
                }
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
            StreamStageKind::Each => {
                let mut parallel = false;
                for option in self.program.arena.stream_options(stage.options) {
                    if option.name.as_str() != "jobs" {
                        return None;
                    }
                    parallel = true;
                }
                let block = stage.block?;
                let (slot, cleanup) = self.lower_pipeline_stage_item_slot(stage, slots, item_ty)?;
                let saved = slots.enter();
                let body =
                    self.lower_block_in_current_scope(block, slots, current_function, Some(slot))?;
                slots.exit(saved);
                cleanup_pipeline_stage_item_slot(slots, cleanup, slot);
                Some(LoweredPipelineStage::Each {
                    slot,
                    body,
                    parallel,
                })
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
            StreamStageKind::TablePrint => {
                if stage.block.is_some() || !stage.options.is_empty() {
                    return None;
                }
                let columns = if stage.args.is_empty() {
                    None
                } else {
                    Some(self.lower_pipeline_stage_table_print_columns(
                        stage,
                        slots,
                        current_function,
                    )?)
                };
                Some(LoweredPipelineStage::TablePrint { columns })
            }
            StreamStageKind::ReduceBy => {
                let mut op = None;
                for option in self.program.arena.stream_options(stage.options) {
                    let selected = match option.name.as_str().as_str() {
                        "sum" => ReduceByOp::Sum,
                        "min" => ReduceByOp::Min,
                        "max" => ReduceByOp::Max,
                        // `--jobs=N` is a parallelism hint; serial execution
                        // produces identical results, so accept and ignore it.
                        "jobs" => continue,
                        _ => return None,
                    };
                    // exactly one of --sum/--min/--max
                    if op.replace(selected).is_some() {
                        return None;
                    }
                }
                self.lower_pipeline_stage_reduce_by(stage, slots, current_function, op?, item_ty)
            }
            StreamStageKind::Shuffle => {
                if !stage.options.is_empty() || stage.block.is_some() {
                    return None;
                }
                let args = self.program.arena.call_args(stage.args);
                let seed = match args {
                    [] => None,
                    [arg] => {
                        let ArenaCallArgKind::Positional(seed) = arg.kind else {
                            return None;
                        };
                        Some(self.lower_expr(seed, slots, current_function, None)?)
                    }
                    _ => return None,
                };
                Some(LoweredPipelineStage::Shuffle { seed })
            }
            StreamStageKind::Fold | StreamStageKind::Reduce => {
                self.lower_pipeline_stage_fold(stage, slots, current_function, item_ty)
            }
        }
    }

    fn lower_pipeline_stage_desc_option(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
    ) -> Option<Option<BuildExprId>> {
        let options = self.program.arena.stream_options(stage.options);
        match options {
            [] => Some(None),
            [option] if option.name == "desc" => match option.value {
                Some(value) => Some(Some(self.lower_expr(
                    value,
                    slots,
                    current_function,
                    None,
                )?)),
                None => Some(Some(push_build_row!(self, expr, BuildExprRow::Bool(true)))),
            },
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

    fn lower_pipeline_stage_table_print_columns(
        &mut self,
        stage: &ArenaStreamStage,
        _slots: &mut SlotScope,
        _current_function: Option<Name>,
    ) -> Option<Vec<String>> {
        let args = self.program.arena.call_args(stage.args);
        let mut columns = Vec::with_capacity(args.len());
        for arg in args {
            let ArenaCallArgKind::Named { name, value, .. } = &arg.kind else {
                return None;
            };
            if name.as_str() != "columns" {
                return None;
            }
            let ArenaExprKind::List(items) = self.program.arena.expr(*value).kind else {
                return None;
            };
            for item in self.program.arena.expr_ids(items) {
                let ArenaExprKind::Str(s) = self.program.arena.expr(item).kind else {
                    return None;
                };
                columns.push(self.program.arena.string_literal(s).to_string());
            }
        }
        Some(columns)
    }

    fn lower_pipeline_stage_fold(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
    ) -> Option<LoweredPipelineStage> {
        if !stage.options.is_empty() {
            return None;
        }
        let [arg] = self.program.arena.call_args(stage.args) else {
            return None;
        };
        let ArenaCallArgKind::Positional(initial) = arg.kind else {
            return None;
        };
        let block = stage.block?;
        let statements = self.program.arena.block(block).statements;
        let ids = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let (&tail, prefix) = ids.split_last()?;
        let initial = self.lower_expr(initial, slots, current_function, None)?;
        let saved = slots.enter();
        let params = self
            .program
            .arena
            .block_params(self.program.arena.block(block).params);
        let acc_slot = match params {
            [] => slots.reserve("pipeline.acc"),
            [acc] | [acc, _] => {
                if slots.is_bound_non_capture(acc.name) {
                    slots.exit(saved);
                    return None;
                }
                slots.declare(acc.name)
            }
            _ => {
                slots.exit(saved);
                return None;
            }
        };
        let item_slot = match params {
            [_, item] => {
                if slots.is_bound_non_capture(item.name) {
                    slots.exit(saved);
                    return None;
                }
                slots.declare_with_type(item.name, item_ty.cloned())
            }
            _ => slots.reserve("pipeline.item"),
        };
        let mut body = Vec::with_capacity(prefix.len());
        for stmt in prefix {
            let Some(lowered) =
                self.lower_stmt_with_blocker_guard(*stmt, slots, current_function, Some(item_slot))
            else {
                slots.exit(saved);
                return None;
            };
            let lowered_row = self.scratch.borrow().statements[lowered.index()].clone();
            if !matches!(
                lowered_row,
                BuildStmtRow::Let { .. }
                    | BuildStmtRow::LetInt { .. }
                    | BuildStmtRow::LetBool { .. }
                    | BuildStmtRow::Assign { .. }
                    | BuildStmtRow::AssignInt { .. }
                    | BuildStmtRow::AssignIndex { .. }
                    | BuildStmtRow::AssignBool { .. }
            ) {
                slots.exit(saved);
                return None;
            }
            body.push(lowered);
        }
        let value = match self.lower_tail_stmt_as_expr(
            tail,
            slots,
            current_function,
            Some(item_slot),
        ) {
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

    fn lower_pipeline_stage_reduce_by(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        op: ReduceByOp,
        item_ty: Option<&Type>,
    ) -> Option<LoweredPipelineStage> {
        // `reduce-by` takes no positional args; the block maps each item to a
        // `{key, value}` record and the runtime aggregates `value` per key.
        if !stage.args.is_empty() {
            return None;
        }
        let block = stage.block?;
        let statements = self.program.arena.block(block).statements;
        let ids = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let (&tail, prefix) = ids.split_last()?;
        let ArenaStmtKind::Expr(expr) = self.program.arena.stmt(tail).kind else {
            return None;
        };
        let saved = slots.enter();
        let params = self
            .program
            .arena
            .block_params(self.program.arena.block(block).params);
        // The block has a single param bound to the item (or none).
        let item_slot = match params {
            [] => slots.reserve("pipeline.item"),
            [item] => {
                if slots.is_bound_non_capture(item.name) {
                    slots.exit(saved);
                    return None;
                }
                slots.declare_with_type(item.name, item_ty.cloned())
            }
            _ => {
                slots.exit(saved);
                return None;
            }
        };
        let mut body = Vec::with_capacity(prefix.len());
        for stmt in prefix {
            let Some(lowered) =
                self.lower_stmt_with_blocker_guard(*stmt, slots, current_function, Some(item_slot))
            else {
                slots.exit(saved);
                return None;
            };
            let lowered_row = self.scratch.borrow().statements[lowered.index()].clone();
            if !matches!(
                lowered_row,
                BuildStmtRow::Let { .. }
                    | BuildStmtRow::LetInt { .. }
                    | BuildStmtRow::LetBool { .. }
                    | BuildStmtRow::Assign { .. }
                    | BuildStmtRow::AssignInt { .. }
                    | BuildStmtRow::AssignIndex { .. }
                    | BuildStmtRow::AssignBool { .. }
            ) {
                slots.exit(saved);
                return None;
            }
            body.push(lowered);
        }
        let value = match self.lower_expr(expr, slots, current_function, Some(item_slot)) {
            Some(value) => value,
            None => {
                slots.exit(saved);
                return None;
            }
        };
        slots.exit(saved);
        Some(LoweredPipelineStage::ReduceBy {
            item_slot,
            body,
            value,
            op,
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
        let [stmt] = statements.as_slice() else {
            return None;
        };
        let (slot, cleanup) = self.lower_pipeline_stage_item_slot(stage, slots, item_ty)?;
        let expr = match self.lower_tail_stmt_as_expr(*stmt, slots, current_function, Some(slot)) {
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
                if slots.is_bound_non_capture(param.name) {
                    return None;
                }
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
        for branch in branches {
            lowered.push((
                self.lower_expr(branch.condition, slots, current_function, item_slot)?,
                self.lower_tail_block(branch.block, slots, current_function, item_slot)?,
            ));
        }
        let else_body = match else_block {
            Some(block) => {
                Some(self.lower_tail_block(block, slots, current_function, item_slot)?)
            }
            None => None,
        };
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
        let (ok_binding_ty, err_binding_ty) =
            self.compact_match_scrutinee_result_types(value, slots);
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
        let (ok_binding_ty, err_binding_ty) =
            self.compact_match_scrutinee_result_types(value, slots);
        let mut lowered_arms = Vec::with_capacity(arms.len());
        for arm in arms {
            if !self.program.arena.block(arm.block).params.is_empty() {
                return None;
            }
            let statements = self.program.arena.block(arm.block).statements;
            let statements = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
            let [stmt] = statements.as_slice() else {
                return None;
            };
            let (pattern, cleanup) = self.lower_pattern(
                arm.pattern,
                slots,
                ok_binding_ty.as_ref(),
                err_binding_ty.as_ref(),
            )?;
            let value = match self.lower_arm_value_expr(*stmt, slots, current_function, item_slot) {
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
            ArenaPatternKind::Literal(expr) => match self.program.arena.expr(expr).kind {
                ArenaExprKind::Str(value) => {
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

    fn pattern_tag_name(&mut self, pattern: PatternId) -> Option<Option<Arc<str>>> {
        let lowered = match self.program.arena.pattern(pattern).kind {
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
        match self.program.arena.stmt(stmt).kind {
            ArenaStmtKind::Expr(expr) => self.lower_expr(expr, slots, current_function, item_slot),
            ArenaStmtKind::TailBareIdent(name) => self.lower_bare_ident(name, slots),
            _ => None,
        }
    }

    /// Lower a block whose sole statement produces a value (a bare expression or
    /// a value-producing tail `if`/`match`) to a single `BuildExprId`. Used by
    /// pipeline stages whose block yields a key/value (`count {…}`,
    /// `sort-by {…}`, etc.) where the block body is written as a bare tail
    /// `if`/`match` statement rather than a parenthesized expression.
    fn lower_block_value_expr(
        &mut self,
        block: BlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let statements = self.program.arena.block(block).statements;
        let statements = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let [stmt] = statements.as_slice() else {
            return None;
        };
        self.lower_tail_stmt_as_expr(*stmt, slots, current_function, item_slot)
    }

    /// Lower a single value-producing tail statement to an expression: a bare
    /// expression, a tail-bare-ident, or a value-producing `if`/`match` whose
    /// branch blocks are themselves single value statements.
    fn lower_tail_stmt_as_expr(
        &mut self,
        stmt: StmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let span = self.program.arena.stmt(stmt).span;
        match self.program.arena.stmt(stmt).kind {
            ArenaStmtKind::Expr(expr) => self.lower_expr(expr, slots, current_function, item_slot),
            ArenaStmtKind::TailBareIdent(name) => self.lower_bare_ident(name, slots),
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                // A value-producing `if` needs an `else`.
                let else_block = else_block?;
                let arena_branches = self.program.arena.if_branches(branches).to_vec();
                let mut lowered = Vec::with_capacity(arena_branches.len());
                for branch in arena_branches {
                    let condition =
                        self.lower_expr(branch.condition, slots, current_function, item_slot)?;
                    let value = self.lower_block_value_expr(
                        branch.block,
                        slots,
                        current_function,
                        item_slot,
                    )?;
                    lowered.push((condition, value));
                }
                let else_value =
                    self.lower_block_value_expr(else_block, slots, current_function, item_slot)?;
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::IfExpr {
                        branches: lowered,
                        else_value,
                        span,
                    }
                ))
            }
            ArenaStmtKind::Match { value, arms } => {
                self.lower_match_stmt_as_expr(value, arms, span, slots, current_function, item_slot)
            }
            _ => None,
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
        let lowered = match &self.program.arena.pattern(id).kind {
            ArenaPatternKind::Wildcard => Some((
                push_build_row!(self, pattern, BuildPatternRow::Wildcard),
                Vec::new(),
            )),
            ArenaPatternKind::Literal(expr) => self
                .lower_pattern_literal(*expr)
                .map(|pattern| (pattern, Vec::new())),
            ArenaPatternKind::Binding(name) if self.compact_tag_variant_arity(*name) == Some(0) => {
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Tag {
                            name: *name,
                            slots: Default::default(),
                        }
                    ),
                    Vec::new(),
                ))
            }
            ArenaPatternKind::Binding(name) if !slots.is_bound_non_capture(*name) => {
                let slot = slots.declare(*name);
                Some((
                    push_build_row!(self, pattern, BuildPatternRow::Bind { slot }),
                    vec![(*name, slot)],
                ))
            }
            ArenaPatternKind::Type {
                binding: Some(name),
                ty,
            } if !slots.is_bound_non_capture(*name) => {
                let lowered_ty = compact_runtime_type(&self.program.arena, *ty, self.declarations);
                let slot = slots.declare(*name);
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
                let lowered_ty = compact_runtime_type(&self.program.arena, *ty, self.declarations);
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
            } => self.lower_error_variant_pattern(*family, *variant, *fields, false, slots),
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
                if name == "Err"
                    && let Some(arg) = arg
                    && let ArenaPatternKind::ErrorVariant {
                        family,
                        variant,
                        fields,
                    } = self.program.arena.pattern(*arg).kind
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
                            name: *name,
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
            ArenaPatternKind::Binding(name) if !slots.is_bound_non_capture(name) => {
                let slot = slots.declare(name);
                cleanup.push((name, slot));
                Some(Some(slot))
            }
            _ => None,
        }
    }

    fn lower_pattern_literal(&self, id: ExprId) -> Option<BuildPatternId> {
        match self.program.arena.expr(id).kind {
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
            ArenaPatternKind::Binding(name) if !slots.is_bound_non_capture(name) => {
                let slot = slots.declare_with_type(name, binding_type.cloned());
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
                if slots.is_bound_non_capture(name) {
                    return None;
                }
                let slot = slots.declare(name);
                cleanup.push((name, slot));
                Some(Some(slot))
            }
            _ => None,
        }
    }

    fn lower_comp_target(
        &self,
        id: BindingTargetId,
        slots: &mut SlotScope,
    ) -> Option<LoweredCompTarget> {
        match self.program.arena.binding_target(id).kind {
            ArenaBindingTargetKind::Name(name) => {
                if slots.is_bound_non_capture(name) {
                    return None;
                }
                Some(LoweredCompTarget::Slot(slots.declare(name)))
            }
            ArenaBindingTargetKind::Record { fields, .. } => {
                let mut lowered = LoweredCompFields::new();
                for field in self.program.arena.destructure_fields(fields) {
                    if slots.is_bound_non_capture(field.name) {
                        return None;
                    }
                    let slot = slots.declare(field.name);
                    lowered.push((field.name, slot, self.program.arena.span(field.span)));
                }
                Some(LoweredCompTarget::Record { fields: lowered })
            }
        }
    }

    fn assign_target_root_name(&self, id: crate::syntax::arena::AssignTargetId) -> Option<Name> {
        match self.program.arena.assign_target(id).kind {
            ArenaAssignTargetKind::Name(name) => Some(name),
            ArenaAssignTargetKind::Field { base, .. }
            | ArenaAssignTargetKind::Index { base, .. } => self.assign_target_root_name(base),
        }
    }

    fn compact_tag_variant_arity(&self, name: Name) -> Option<usize> {
        self.declarations
            .tag_variants_by_name
            .get(&name)
            .map(|variant| variant.field_count)
    }
}

fn construct_top_level_stmt_is_skippable(program: &ArenaProgram, id: StmtId) -> bool {
    if construct_is_main_at_args_call(program, id) {
        return true;
    }
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Export(inner) => construct_top_level_stmt_is_skippable(program, inner),
        ArenaStmtKind::Use(use_id) => construct_use_stmt_is_skippable(program, use_id),
        ArenaStmtKind::Expr(expr) if construct_expr_is_reveal_type_call(program, expr) => true,
        ArenaStmtKind::TypeDef(_)
        | ArenaStmtKind::ErrorDef(_)
        | ArenaStmtKind::ProcDef(_)
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

fn compact_runtime_type(
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
) -> Type {
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
        ArenaTypeExprTag::Named => {
            let name = Name::from_symbol(Symbol::from_raw(data.lhs));
            if let Some(builtin) = BuiltinTypeName::parse(&name.as_str()) {
                return Type::from_builtin_name(builtin);
            }
            if let Some(record) = standard_record_type(&name.as_str()) {
                return record;
            }
            match declarations.types.get(&name) {
                Some(CompactTypeDefInfo::Alias(alias)) => {
                    compact_runtime_type_inner(arena, *alias, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Record(_)) => compact_record_type(name, declarations),
                Some(CompactTypeDefInfo::Module(exports)) => Type::Module(exports.clone()),
                Some(CompactTypeDefInfo::TagUnion) => Type::Tag(name),
                None => Type::Record(Default::default()),
            }
        }
        ArenaTypeExprTag::Qualified => {
            let name = Name::from_symbol(Symbol::from_raw(data.rhs));
            match declarations.types.get(&name) {
                Some(CompactTypeDefInfo::Alias(alias)) => {
                    compact_runtime_type_inner(arena, *alias, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Record(_)) => compact_record_type(name, declarations),
                Some(CompactTypeDefInfo::Module(exports)) => Type::Module(exports.clone()),
                Some(CompactTypeDefInfo::TagUnion) => Type::Tag(name),
                None => Type::Record(Default::default()),
            }
        }
        ArenaTypeExprTag::List => Type::List(Box::new(compact_runtime_type_inner(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        ))),
        ArenaTypeExprTag::Map => Type::Map(Box::new(compact_runtime_type_inner(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        ))),
        ArenaTypeExprTag::Stream => Type::Stream(Box::new(compact_runtime_type_inner(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        ))),
        ArenaTypeExprTag::Module => Type::Module(Default::default()),
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
    }
}

fn compact_record_type(name: Name, declarations: &CompactDeclOutput) -> Type {
    let Some(CompactTypeDefInfo::Record(fields)) = declarations.types.get(&name) else {
        return Type::Unknown;
    };
    Type::Record(fields.clone())
}

fn compact_type_check(
    kind: LoweredType,
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
) -> Option<LoweredTypeCheck> {
    lowered_type_needs_static_check(kind).then(|| LoweredTypeCheck {
        ty: compact_runtime_type(arena, ty, declarations),
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
        ArenaTypeExprTag::Map => format!(
            "Map[{}]",
            compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize))
        ),
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
        Type::Int => Some(LoweredType::Int),
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
        Type::Pure => Some(LoweredType::Pure),
        Type::Proc => Some(LoweredType::Proc),
        Type::Error | Type::ErrorFamily(_) | Type::ErrorVariant { .. } => Some(LoweredType::Error),
        Type::Record(_) => Some(LoweredType::Record),
        Type::Module(_) => Some(LoweredType::Module),
        Type::List(_) => Some(LoweredType::List),
        Type::Stream(_) => Some(LoweredType::Stream),
        Type::Map(_) => Some(LoweredType::Map),
        Type::Tag(_) => Some(LoweredType::Tag),
        Type::Result(_, _) => Some(LoweredType::Result),
        Type::Any | Type::Unknown | Type::Invalid => Some(LoweredType::Any),
        _ => None,
    }
}

fn lowered_method_supported_for_type(ty: &Type, name: Name, arg_count: usize) -> bool {
    match ty {
        Type::Any | Type::Unknown => lowered_method_name(&name.as_str()),
        Type::Invalid => true,
        Type::Optional(inner) => lowered_method_supported_for_type(inner, name, arg_count),
        Type::Result(ok, _) => {
            name == "context" && (arg_count == 1 || arg_count == 2)
                || lowered_method_supported_for_type(ok, name, arg_count)
        }
        Type::Int => name == "float" && arg_count == 0,
        Type::Float => match name.as_str().as_str() {
            "floor" | "ceil" | "round" | "sqrt" | "exp" | "ln" | "sin" | "cos" | "tan" | "abs" => {
                arg_count == 0
            }
            "format" => arg_count <= 1,
            "pow" | "log" => arg_count == 1,
            _ => false,
        },
        Type::Str => match name.as_str().as_str() {
            "trim" | "lower" | "upper" | "reverse" | "lines" | "words" | "parse_int"
            | "parse_float" | "base64_decode" | "base32_decode" | "count_lines" | "count_words"
            | "count_chars" | "count_bytes" | "byte_len" => arg_count == 0,
            "fields" | "squeeze" => arg_count <= 1,
            "split" => arg_count == 1 || arg_count == 2,
            "wrap" | "delete" | "starts_with" | "ends_with" | "contains" => arg_count == 1,
            "replace" | "translate" => arg_count == 2,
            "byte_at" | "byte_slice" | "find" => arg_count == 1 || arg_count == 2,
            _ => false,
        },
        Type::Bytes => match name.as_str().as_str() {
            "trim" | "lines" | "count_lines" | "len" | "lower" | "base64" | "base32" | "md5"
            | "sha1" | "sha256" | "sha512" | "utf8" => arg_count == 0,
            "dump" | "strings" => arg_count <= 1,
            "chunks" | "compare" | "starts_with" | "ends_with" | "contains" => arg_count == 1,
            "byte_at" | "slice" => arg_count == 1 || arg_count == 2,
            _ => false,
        },
        Type::Digest => matches!(name.as_str().as_str(), "hex" | "base64") && arg_count == 0,
        Type::Regex => match name.as_str().as_str() {
            "matches" | "find" | "captures" => arg_count == 1,
            "replace" => arg_count == 2,
            _ => false,
        },
        Type::Status => match name.as_str().as_str() {
            "exited" | "signaled" | "exit_code" | "signal_number" => arg_count == 0,
            "exited_with" => arg_count == 1,
            _ => false,
        },
        Type::Path => match name.as_str().as_str() {
            "display" | "name" | "ext" | "normalize" | "parent" | "lines" | "bytes_lines"
            | "read_text" | "read_bytes" | "exists" | "executable" | "du" | "metadata"
            | "readlink" | "resolve" | "remove_dir" | "unlink" => arg_count == 0,
            "with_ext" | "strip_prefix" | "relative_to" | "touch_from" | "truncate" | "chmod"
            | "hardlink" | "write" | "write_atomic" => arg_count == 1,
            "copy" | "rename" | "mkdir" | "remove" => arg_count == 1 || arg_count == 2,
            "touch" => arg_count <= 1,
            _ => false,
        },
        Type::Record(_) | Type::Module(_) => {
            matches!(name.as_str().as_str(), "has" | "get") && arg_count == 1
                || matches!(name.as_str().as_str(), "keys" | "len") && arg_count == 0
        }
        Type::List(_) => match name.as_str().as_str() {
            "collect" | "len" => arg_count == 0,
            "contains" | "push" | "extend" => arg_count == 1,
            "get" => arg_count == 1 || arg_count == 2,
            "join" => arg_count <= 1,
            _ => false,
        },
        Type::Map(_) => match name.as_str().as_str() {
            "len" | "keys" | "values" => arg_count == 0,
            "has" | "remove" => arg_count == 1,
            "get" => arg_count == 1 || arg_count == 2,
            "set" | "push" => arg_count == 2,
            _ => false,
        },
        Type::ProcessHandle => name == "cancel" && arg_count <= 2,
        Type::Stream(_) => name == "collect" && arg_count == 0,
        _ => false,
    }
}

fn infer_checked_method_return_type(receiver: &Type, name: Name) -> Option<Type> {
    match receiver {
        Type::Optional(inner) => infer_checked_method_return_type(inner, name),
        Type::Result(ok, err) => {
            if name == "context" {
                Some(Type::Result(ok.clone(), err.clone()))
            } else {
                infer_checked_method_return_type(ok, name)
            }
        }
        Type::Str => match name.as_str().as_str() {
            "trim" | "lower" | "upper" | "reverse" | "format" | "replace" | "translate"
            | "delete" | "squeeze" | "byte_slice" | "slice" => Some(Type::Str),
            "lines" | "words" | "fields" | "split" | "wrap" => {
                Some(Type::List(Box::new(Type::Str)))
            }
            "base64_decode" | "base32_decode" => {
                Some(Type::Result(Box::new(Type::Bytes), Box::new(Type::Error)))
            }
            "parse_int" => Some(Type::Result(Box::new(Type::Int), Box::new(Type::Error))),
            "parse_float" => Some(Type::Result(Box::new(Type::Float), Box::new(Type::Error))),
            "count_lines" | "count_words" | "count_chars" | "count_bytes" | "byte_len"
            | "byte_at" | "find" => Some(Type::Int),
            "starts_with" | "ends_with" | "contains" => Some(Type::Bool),
            _ => None,
        },
        Type::Bytes => match name.as_str().as_str() {
            "trim" | "lower" | "slice" => Some(Type::Bytes),
            "base64" | "base32" | "dump" => Some(Type::Str),
            "strings" => Some(Type::List(Box::new(Type::Str))),
            "lines" => Some(Type::Stream(Box::new(Type::Bytes))),
            "chunks" => Some(Type::List(Box::new(Type::Bytes))),
            "utf8" => Some(Type::Result(Box::new(Type::Str), Box::new(Type::Error))),
            "compare" => {
                let mut fields = BTreeMap::new();
                fields.insert(Name::intern("equal"), Type::Bool);
                fields.insert(Name::intern("byte"), Type::Int);
                fields.insert(Name::intern("line"), Type::Int);
                fields.insert(Name::intern("left"), Type::Int);
                fields.insert(Name::intern("right"), Type::Int);
                Some(Type::Record(fields))
            }
            "count_lines" | "len" | "byte_at" => Some(Type::Int),
            "starts_with" | "ends_with" | "contains" => Some(Type::Bool),
            "md5" | "sha1" | "sha256" | "sha512" => Some(Type::Digest),
            _ => None,
        },
        Type::Digest => match name.as_str().as_str() {
            "hex" | "base64" => Some(Type::Str),
            _ => None,
        },
        Type::Regex => match name.as_str().as_str() {
            "matches" => Some(Type::Bool),
            "find" => standard_record_type("RegexMatch").map(|ty| Type::List(Box::new(ty))),
            "captures" => Some(Type::List(Box::new(Type::Str))),
            "replace" => Some(Type::Str),
            _ => None,
        },
        Type::Path => match name.as_str().as_str() {
            "display" | "name" | "ext" => Some(Type::Str),
            "normalize" | "parent" | "relative_to" | "with_ext" => Some(Type::Path),
            "strip_prefix" | "readlink" | "resolve" => {
                Some(Type::Result(Box::new(Type::Path), Box::new(Type::Error)))
            }
            "read_text" => Some(Type::Result(Box::new(Type::Str), Box::new(Type::Error))),
            "read_bytes" => Some(Type::Result(Box::new(Type::Bytes), Box::new(Type::Error))),
            "exists" | "executable" => {
                Some(Type::Result(Box::new(Type::Bool), Box::new(Type::Error)))
            }
            "du" => Some(Type::Result(Box::new(Type::Int), Box::new(Type::Error))),
            "metadata" => standard_record_type("FsEntry")
                .map(|ty| Type::Result(Box::new(ty), Box::new(Type::Error))),
            "mkdir" | "remove" | "write" | "write_atomic" | "copy" | "rename" | "remove_dir"
            | "touch" | "touch_from" | "truncate" | "chmod" | "hardlink" | "unlink" => {
                Some(Type::Result(Box::new(Type::Unit), Box::new(Type::Error)))
            }
            _ => None,
        },
        Type::List(item) => match name.as_str().as_str() {
            "collect" => Some(Type::List(item.clone())),
            "len" => Some(Type::Int),
            "get" => Some(Type::Result(item.clone(), Box::new(Type::Error))),
            "push" | "extend" => Some(Type::List(item.clone())),
            "join" => Some(Type::Str),
            "contains" => Some(Type::Bool),
            _ => None,
        },
        Type::Map(item) => match name.as_str().as_str() {
            "keys" => Some(Type::List(Box::new(Type::Str))),
            "values" => Some(Type::List(item.clone())),
            "len" => Some(Type::Int),
            "get" => Some(Type::Result(item.clone(), Box::new(Type::Error))),
            "set" | "remove" => Some(Type::Map(item.clone())),
            "has" => Some(Type::Bool),
            _ => None,
        },
        Type::Record(fields) => match name.as_str().as_str() {
            "keys" => Some(Type::List(Box::new(Type::Str))),
            "len" => Some(Type::Int),
            "has" => Some(Type::Bool),
            "get" => Some(Type::Result(Box::new(Type::Any), Box::new(Type::Error))),
            "values" => Some(Type::List(Box::new(
                fields.values().next().cloned().unwrap_or(Type::Any),
            ))),
            _ => None,
        },
        Type::Module(_) => match name.as_str().as_str() {
            "keys" => Some(Type::List(Box::new(Type::Str))),
            "len" => Some(Type::Int),
            "has" => Some(Type::Bool),
            "get" => Some(Type::Result(Box::new(Type::Any), Box::new(Type::Error))),
            _ => None,
        },
        Type::ProcessHandle if name == "cancel" => {
            Some(Type::Result(Box::new(Type::Unit), Box::new(Type::Error)))
        }
        _ => None,
    }
}

fn module_export_call_return_type(ty: Type, name: Name) -> Option<Type> {
    let Type::Module(exports) = ty else {
        return None;
    };
    match exports.get(&name)? {
        ModuleExportType::Proc { sig, .. } | ModuleExportType::Pure { sig, .. } => {
            Some(sig.return_ty.as_ref().clone())
        }
        ModuleExportType::Value { .. } => None,
    }
}

fn lowered_result_fallback_type(ty: &Type) -> Option<LoweredType> {
    ty.result_ok()
        .or_else(|| ty.optional_inner())
        .and_then(lowered_checked_type)
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
        BuiltinTypeName::Path => Some(LoweredType::Path),
        BuiltinTypeName::Command => Some(LoweredType::Command),
        BuiltinTypeName::ProcessHandle => Some(LoweredType::ProcessHandle),
        BuiltinTypeName::Pure => Some(LoweredType::Pure),
        BuiltinTypeName::Proc => Some(LoweredType::Proc),
        BuiltinTypeName::Error => Some(LoweredType::Error),
        BuiltinTypeName::Record => Some(LoweredType::Record),
        BuiltinTypeName::Module => Some(LoweredType::Module),
        BuiltinTypeName::Result => Some(LoweredType::Result),
        BuiltinTypeName::Null
        | BuiltinTypeName::Map
        | BuiltinTypeName::EnvPathList
        | BuiltinTypeName::ProcessError => None,
    }
}

fn top_level_known_with_runtime_bindings() -> FxHashMap<Name, LoweredTopLevelBinding> {
    let mut known = FxHashMap::default();
    let args = LoweredTopLevelBinding {
        kind: LoweredType::List,
        result_ok: None,
        checked: None,
        mutable: false,
        slot: true,
    };
    known.insert(Name::intern("args"), args.clone());
    known.insert(Name::intern("ARGV"), args);
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
        LoweredType::Pure => Some(Type::Pure),
        LoweredType::Proc => Some(Type::Proc),
        LoweredType::Error => Some(Type::Error),
        LoweredType::Record => Some(Type::Record(Default::default())),
        LoweredType::Module => Some(Type::Module(Default::default())),
        LoweredType::List => Some(Type::List(Box::new(Type::Any))),
        LoweredType::Stream => Some(Type::Stream(Box::new(Type::Any))),
        LoweredType::Map => Some(Type::Map(Box::new(Type::Any))),
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

fn lowerable_top_level_annotation(ty: LoweredType) -> bool {
    matches!(
        ty,
        LoweredType::Unit
            | LoweredType::Int
            | LoweredType::Float
            | LoweredType::Duration
            | LoweredType::Bool
            | LoweredType::Str
            | LoweredType::Bytes
            | LoweredType::Digest
            | LoweredType::Regex
            | LoweredType::Status
            | LoweredType::Path
            | LoweredType::Command
            | LoweredType::ProcessHandle
            | LoweredType::Stream
            | LoweredType::Pure
            | LoweredType::Proc
            | LoweredType::Error
            | LoweredType::Record
            | LoweredType::List
            | LoweredType::Map
            | LoweredType::Tag
            | LoweredType::Result
            | LoweredType::Any
    )
}

fn lowered_builtin_call_ok_type(module: Name, name: Name) -> Option<LoweredType> {
    if module == "regex" && name == "compile" {
        return Some(LoweredType::Regex);
    }
    if module == "fs" && name == "tempdir" {
        return Some(LoweredType::Record);
    }
    if module == "fs" && name == "root_path" {
        return Some(LoweredType::Path);
    }
    if module == "fs" && (name == "write" || name == "mkdir" || name == "remove") {
        return Some(LoweredType::Unit);
    }
    if module == "archive" && name == "tar_create" {
        return Some(LoweredType::Unit);
    }
    if module == "archive" && name == "tar_list" {
        return Some(LoweredType::List);
    }
    if module == "archive" && name == "tar_extract" {
        return Some(LoweredType::Unit);
    }
    if module == "json" && name == "encode" {
        return Some(LoweredType::Str);
    }
    if module == "json" && name == "decode" {
        return Some(LoweredType::Any);
    }
    None
}

fn lowered_result_method_ok_type(name: Name) -> Option<LoweredType> {
    if name == "read_text" {
        return Some(LoweredType::Str);
    }
    if name == "read_bytes" {
        return Some(LoweredType::Bytes);
    }
    if name == "exists" {
        return Some(LoweredType::Bool);
    }
    if name == "executable" {
        return Some(LoweredType::Bool);
    }
    if name == "du" {
        return Some(LoweredType::Int);
    }
    if name == "metadata" {
        return Some(LoweredType::Record);
    }
    if name == "readlink" || name == "resolve" {
        return Some(LoweredType::Path);
    }
    if name == "mkdir"
        || name == "remove"
        || name == "write"
        || name == "write_atomic"
        || name == "copy"
        || name == "rename"
        || name == "remove_dir"
        || name == "touch"
        || name == "touch_from"
        || name == "truncate"
        || name == "chmod"
        || name == "hardlink"
        || name == "unlink"
    {
        return Some(LoweredType::Unit);
    }
    if name == "strip_prefix" {
        return Some(LoweredType::Path);
    }
    if name == "parse_int" {
        return Some(LoweredType::Int);
    }
    if name == "parse_float" {
        return Some(LoweredType::Float);
    }
    if name == "utf8" {
        return Some(LoweredType::Str);
    }
    if name == "base64_decode" || name == "base32_decode" {
        return Some(LoweredType::Bytes);
    }
    if name == "cancel" {
        return Some(LoweredType::Unit);
    }
    if name == "floor" || name == "ceil" || name == "round" {
        return Some(LoweredType::Int);
    }
    if name == "exit_code" || name == "signal_number" {
        return Some(LoweredType::Int);
    }
    None
}

fn lowered_plain_method_type(name: Name) -> Option<LoweredType> {
    if name == "count_lines"
        || name == "count_words"
        || name == "count_chars"
        || name == "count_bytes"
        || name == "byte_len"
        || name == "byte_at"
        || name == "len"
        || name == "find"
    {
        return Some(LoweredType::Int);
    }
    if name == "float"
        || name == "sqrt"
        || name == "pow"
        || name == "exp"
        || name == "ln"
        || name == "log"
        || name == "sin"
        || name == "cos"
        || name == "tan"
        || name == "abs"
    {
        return Some(LoweredType::Float);
    }
    if name == "exited"
        || name == "signaled"
        || name == "exited_with"
        || name == "starts_with"
        || name == "ends_with"
        || name == "contains"
        || name == "has"
        || name == "matches"
    {
        return Some(LoweredType::Bool);
    }
    if name == "display"
        || name == "name"
        || name == "ext"
        || name == "byte_slice"
        || name == "slice"
        || name == "trim"
        || name == "lower"
        || name == "upper"
        || name == "reverse"
        || name == "replace"
        || name == "translate"
        || name == "delete"
        || name == "squeeze"
        || name == "format"
        || name == "hex"
        || name == "base64"
        || name == "base32"
        || name == "join"
        || name == "dump"
    {
        return Some(LoweredType::Str);
    }
    if name == "words"
        || name == "fields"
        || name == "split"
        || name == "captures"
        || name == "collect"
        || name == "keys"
        || name == "values"
        || name == "push"
        || name == "extend"
        || name == "wrap"
        || name == "strings"
        || name == "chunks"
    {
        return Some(LoweredType::List);
    }
    if name == "remove" {
        return Some(LoweredType::Map);
    }
    if name == "normalize" || name == "parent" || name == "relative_to" || name == "with_ext" {
        return Some(LoweredType::Path);
    }
    if name == "format" {
        return Some(LoweredType::Str);
    }
    if name == "base64_decode" || name == "base32_decode" {
        return Some(LoweredType::Result);
    }
    if name == "sha256" {
        return Some(LoweredType::Digest);
    }
    if name == "compare" {
        return Some(LoweredType::Record);
    }
    None
}

impl CompactLowerConstructProbe<'_, '_> {
    fn lower_int_expr_candidate(&self, expr: &BuildExprId) -> Option<BuildIntId> {
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
            BuildExprRow::StrByteAt {
                receiver,
                index,
                default,
                span,
            } => {
                let receiver_row = {
                    let scratch = self.scratch.borrow();
                    scratch.expressions[receiver.index()].clone()
                };
                match receiver_row {
                    BuildExprRow::Param(slot) => Some(push_build_row!(
                        self,
                        int,
                        BuildIntRow::StrByteAtSlot {
                            slot,
                            index: self.lower_int_expr_candidate(index)?,
                            default: match default {
                                Some(value) => Some(self.lower_int_expr_candidate(value)?),
                                None => None,
                            },
                            span: *span,
                        }
                    )),
                    _ => None,
                }
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
                op, left, right, ..
            } => match op {
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
            BuildExprRow::Contains {
                receiver,
                needle,
                span,
            } => {
                let receiver_row = self.scratch.borrow().expressions[receiver.index()].clone();
                let BuildExprRow::Param(slot) = receiver_row else {
                    return None;
                };
                let needle_row = self.scratch.borrow().expressions[needle.index()].clone();
                if let BuildExprRow::Str(needle) = needle_row {
                    return Some(push_build_row!(
                        self,
                        bool,
                        BuildBoolRow::StrContainsSlot {
                            slot,
                            needle,
                            span: *span,
                        }
                    ));
                }
                let needle = self.lowered_literal_value(needle)?;
                Some(push_build_row!(
                    self,
                    bool,
                    BuildBoolRow::ContainsSlot {
                        slot,
                        needle,
                        span: *span,
                    }
                ))
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
            receiver,
            index,
            default,
            ..
        } = self.scratch.borrow().expressions[value.index()].clone()
        else {
            return false;
        };
        if self.scan_bytes_expr_slot(receiver) != Some(line_slot) || default.is_some() {
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
            BuildStmtRow::Yield { .. } => false,
            BuildStmtRow::ScanLines { .. } => false,
            BuildStmtRow::ScanBytes { .. } => false,
            BuildStmtRow::Break | BuildStmtRow::BreakValue { .. } => false,
            BuildStmtRow::Continue => false,
            BuildStmtRow::If {
                branches,
                else_body,
            } => {
                branches
                    .iter()
                    .all(|(_, body)| lowered_body_can_return(scratch, body))
                    && else_body
                        .as_ref()
                        .is_some_and(|body| lowered_body_can_return(scratch, body))
            }
            BuildStmtRow::IfBool {
                branches,
                else_body,
            } => {
                branches
                    .iter()
                    .all(|(_, body)| lowered_body_can_return(scratch, body))
                    && else_body
                        .as_ref()
                        .is_some_and(|body| lowered_body_can_return(scratch, body))
            }
            BuildStmtRow::While { body, .. }
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
            BuildStmtRow::Let { .. }
            | BuildStmtRow::LetRecord { .. }
            | BuildStmtRow::LetInt { .. }
            | BuildStmtRow::LetBool { .. }
            | BuildStmtRow::Assign { .. }
            | BuildStmtRow::AssignInt { .. }
            | BuildStmtRow::AssignField { .. }
            | BuildStmtRow::AssignFieldInt { .. }
            | BuildStmtRow::AssignIndex { .. }
            | BuildStmtRow::AssignBool { .. }
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
        LoweredReturnKind::Plain(LoweredType::Unit) | LoweredReturnKind::Result(LoweredType::Unit)
    )
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
) -> bool {
    match value {
        Value::Error(error) => {
            error.family_name() == *family
                && error.variant_name() == *variant
                && lowered_error_pattern_fields_match(&error.payload, fields, slots)
        }
        Value::RunError(error) if *family == Name::PROCESS_ERROR => {
            if error.variant_name() != variant.as_str() {
                return false;
            }
            let payload = error.payload();
            lowered_error_pattern_fields_match(&payload, fields, slots)
        }
        _ => false,
    }
}

fn lowered_error_pattern_fields_match(
    payload: &RecordMap,
    fields: &LoweredErrorPatternFields,
    slots: &mut [LoweredValue],
) -> bool {
    for (name, slot) in fields {
        let Some(value) = payload.get(&name.as_str()) else {
            return false;
        };
        if let Some(slot) = slot {
            let Some(value) = lowered_value_from_runtime_any(value) else {
                return false;
            };
            slots[*slot] = value;
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

pub(super) fn lowered_sum_records(mut acc: LoweredValue, val: LoweredValue) -> LoweredValue {
    match (&mut acc, val) {
        (LoweredValue::Record(acc_map), LoweredValue::Record(val_map)) => {
            for (key, value) in val_map {
                if let Some(acc_value) = acc_map.get_mut(&key) {
                    *acc_value =
                        lowered_sum_values(std::mem::replace(acc_value, LoweredValue::Unit), value);
                } else {
                    acc_map.insert(key, value);
                }
            }
        }
        (LoweredValue::RecordVec(acc_map), LoweredValue::RecordVec(val_map)) => {
            for (key, value) in val_map {
                if let Some(acc_value) = lowered_record_vec_get_mut(acc_map, &key.as_str()) {
                    *acc_value =
                        lowered_sum_values(std::mem::replace(acc_value, LoweredValue::Unit), value);
                } else {
                    lowered_record_vec_insert(acc_map, key, value);
                }
            }
        }
        (LoweredValue::Record(acc_map), LoweredValue::RecordVec(val_map)) => {
            for (key, value) in val_map {
                let key_text = key.as_str();
                if let Some(acc_value) = acc_map.get_mut::<str>(key_text.as_str()) {
                    *acc_value =
                        lowered_sum_values(std::mem::replace(acc_value, LoweredValue::Unit), value);
                } else {
                    acc_map.insert(Arc::<str>::from(key_text.as_str()), value);
                }
            }
        }
        (LoweredValue::RecordVec(acc_map), LoweredValue::Record(val_map)) => {
            for (key, value) in val_map {
                if let Some(acc_value) = lowered_record_vec_get_mut(acc_map, key.as_ref()) {
                    *acc_value =
                        lowered_sum_values(std::mem::replace(acc_value, LoweredValue::Unit), value);
                } else {
                    lowered_record_vec_insert(acc_map, Name::intern(key.as_ref()), value);
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
        (LoweredValue::Record(mut acc_map), LoweredValue::Record(val_map)) => {
            for (key, value) in val_map {
                if let Some(acc_value) = acc_map.get_mut(&key) {
                    *acc_value =
                        lowered_sum_values(std::mem::replace(acc_value, LoweredValue::Unit), value);
                } else {
                    acc_map.insert(key, value);
                }
            }
            LoweredValue::Record(acc_map)
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
        StmtFlow::Return(value) => Flow::Return(value.into_value()),
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
