#![allow(clippy::single_call_fn)]

use super::{
    ApiArgCheck, MethodReceiver, MethodReceiverSig, MethodSig, NamedMethodSigs,
    ParamSig, RuntimeOp, Type, btree_map, default_param, fs_entry_type, net_response_type, param,
    regex_match_type, result, sig_with_arg_check, fs_root_type, fs_root_read_result_type,
    fs_root_filesystem_stats_type, fs_root_children_result_type, fs_root_readlink_result_type,
};
use crate::types::BuiltinTypeParameter;

pub(in crate::signature) fn value_methods() -> Vec<MethodReceiverSig> {
    let mut receivers = vec![
        MethodReceiverSig {
            receiver: MethodReceiver::PathConstructor,
            methods: method_map(vec![method(
                "parse_bytes",
                vec![param("bytes", Type::Bytes)],
                result(Type::Path),
                true,
                RuntimeOp::PathParseBytes,
            )]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::Result,
            methods: method_map(vec![method_with_arg_check(
                "context",
                vec![
                    param("kind", Type::Str),
                    default_param("message", Type::Str),
                ],
                Type::BuiltinParameter(BuiltinTypeParameter::Receiver),
                true,
                RuntimeOp::ResultContext,
                ApiArgCheck::ResultContext,
            )]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::EnvPathList,
            methods: method_map(vec![
                method_with_arg_check(
                    "prepend",
                    vec![param("path", Type::Path)],
                    result(Type::Unit),
                    false,
                    RuntimeOp::EnvPathPrepend,
                    ApiArgCheck::PathLikeSingle,
                ),
                method_with_arg_check(
                    "append",
                    vec![param("path", Type::Path)],
                    result(Type::Unit),
                    false,
                    RuntimeOp::EnvPathAppend,
                    ApiArgCheck::PathLikeSingle,
                ),
                method(
                    "pop",
                    Vec::new(),
                    result(Type::Path),
                    false,
                    RuntimeOp::EnvPathPop,
                ),
            ]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::Path,
            methods: method_map(vec![
                method(
                    "display",
                    Vec::new(),
                    Type::Str,
                    true,
                    RuntimeOp::PathDisplay,
                ),
                method(
                    "name",
                    Vec::new(),
                    Type::Str,
                    true,
                    RuntimeOp::PathName,
                ),
                method(
                    "basename",
                    Vec::new(),
                    Type::Str,
                    true,
                    RuntimeOp::PathName,
                ),
                method(
                    "dirname",
                    Vec::new(),
                    Type::Path,
                    true,
                    RuntimeOp::PathParent,
                ),
                method(
                    "ext",
                    Vec::new(),
                    Type::Str,
                    true,
                    RuntimeOp::PathExt,
                ),
                method(
                    "ext_or",
                    vec![param("fallback", Type::Str)],
                    Type::Str,
                    true,
                    RuntimeOp::PathExt,
                ),
                method(
                    "normalize",
                    Vec::new(),
                    Type::Path,
                    true,
                    RuntimeOp::PathNormalize,
                ),
                method(
                    "parent",
                    Vec::new(),
                    Type::Path,
                    true,
                    RuntimeOp::PathParent,
                ),
                method(
                    "resolve",
                    Vec::new(),
                    result(Type::Path),
                    false,
                    RuntimeOp::PathResolve,
                ),
                method_with_arg_check(
                    "strip_prefix",
                    vec![param("prefix", Type::Path)],
                    result(Type::Path),
                    true,
                    RuntimeOp::PathStripPrefix,
                    ApiArgCheck::PathLikeSingle,
                ),
                method(
                    "relative_to",
                    vec![param("base", Type::Path)],
                    Type::Path,
                    true,
                    RuntimeOp::PathRelativeTo,
                ),
                method(
                    "with_ext",
                    vec![param("ext", Type::Str)],
                    Type::Path,
                    true,
                    RuntimeOp::PathWithExt,
                ),
                method(
                    "exists",
                    Vec::new(),
                    result(Type::Bool),
                    false,
                    RuntimeOp::FsExists,
                ),
                method(
                    "executable",
                    Vec::new(),
                    result(Type::Bool),
                    false,
                    RuntimeOp::FsExecutable,
                ),
                method(
                    "du",
                    Vec::new(),
                    result(Type::Int),
                    false,
                    RuntimeOp::FsDu,
                ),
                method(
                    "metadata",
                    Vec::new(),
                    result(fs_entry_type()),
                    false,
                    RuntimeOp::FsMetadata,
                ),
                method(
                    "read_bytes",
                    Vec::new(),
                    result(Type::Bytes),
                    false,
                    RuntimeOp::FsRead,
                ),
                method(
                    "read_text",
                    Vec::new(),
                    result(Type::Str),
                    false,
                    RuntimeOp::FsReadText,
                ),
                method(
                    "lines",
                    Vec::new(),
                    result(Type::Stream(Box::new(Type::Str))),
                    false,
                    RuntimeOp::FsStreamLines,
                ),
                method(
                    "bytes_lines",
                    Vec::new(),
                    result(Type::Stream(Box::new(Type::Bytes))),
                    false,
                    RuntimeOp::FsBytesLines,
                ),
                method(
                    "write",
                    vec![param("data", Type::Bytes)],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsWrite,
                ),
                method(
                    "write",
                    vec![param("data", Type::Str)],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsWrite,
                ),
                method(
                    "write_atomic",
                    vec![param("data", Type::Bytes)],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsWriteAtomic,
                ),
                method(
                    "write_atomic",
                    vec![param("data", Type::Str)],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsWriteAtomic,
                ),
                method(
                    "copy",
                    vec![
                        param("dest", Type::Path),
                        default_param("overwrite", Type::Bool),
                    ],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsCopy,
                ),
                method(
                    "rename",
                    vec![
                        param("dest", Type::Path),
                        default_param("overwrite", Type::Bool),
                    ],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsRename,
                ),
                method(
                    "mkdir",
                    vec![default_param("parents", Type::Bool)],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsMkdir,
                ),
                method(
                    "remove",
                    vec![default_param("missing_ok", Type::Bool)],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsRemove,
                ),
                method(
                    "remove_dir",
                    Vec::new(),
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsRemoveDir,
                ),
                method(
                    "touch",
                    vec![default_param("create", Type::Bool)],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsTouch,
                ),
                method(
                    "touch_from",
                    vec![param("reference", Type::Path)],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsTouchFrom,
                ),
                method(
                    "truncate",
                    vec![param("size", Type::Int)],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsTruncate,
                ),
                method(
                    "chmod",
                    vec![param("mode", Type::Int)],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsChmod,
                ),
                method(
                    "hardlink",
                    vec![param("path", Type::Path)],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsHardlink,
                ),
                method(
                    "unlink",
                    Vec::new(),
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsUnlink,
                ),
                method(
                    "readlink",
                    Vec::new(),
                    result(Type::Path),
                    false,
                    RuntimeOp::FsReadlink,
                ),
            ]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::Int,
            methods: method_map(vec![
                method(
                    "float",
                    Vec::new(),
                    Type::Float,
                    true,
                    RuntimeOp::IntFloat,
                ),
                method(
                    "bit_and",
                    vec![param("mask", Type::Int)],
                    Type::Int,
                    true,
                    RuntimeOp::IntBitAnd,
                ),
                method(
                    "bit_or",
                    vec![param("mask", Type::Int)],
                    Type::Int,
                    true,
                    RuntimeOp::IntBitOr,
                ),
                method(
                    "clear_bits",
                    vec![param("mask", Type::Int)],
                    Type::Int,
                    true,
                    RuntimeOp::IntClearBits,
                ),
            ]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::Float,
            methods: method_map(vec![
                method(
                    "floor",
                    Vec::new(),
                    result(Type::Int),
                    true,
                    RuntimeOp::FloatFloor,
                ),
                method(
                    "ceil",
                    Vec::new(),
                    result(Type::Int),
                    true,
                    RuntimeOp::FloatCeil,
                ),
                method(
                    "round",
                    Vec::new(),
                    result(Type::Int),
                    true,
                    RuntimeOp::FloatRound,
                ),
                method(
                    "format",
                    vec![default_param("precision", Type::Int)],
                    Type::Str,
                    true,
                    RuntimeOp::FloatFormat,
                ),
                method(
                    "sqrt",
                    Vec::new(),
                    Type::Float,
                    true,
                    RuntimeOp::FloatSqrt,
                ),
                method(
                    "pow",
                    vec![param("exp", Type::Float)],
                    Type::Float,
                    true,
                    RuntimeOp::FloatPow,
                ),
                method(
                    "exp",
                    Vec::new(),
                    Type::Float,
                    true,
                    RuntimeOp::FloatExp,
                ),
                method(
                    "ln",
                    Vec::new(),
                    Type::Float,
                    true,
                    RuntimeOp::FloatLn,
                ),
                method(
                    "log",
                    vec![param("base", Type::Float)],
                    Type::Float,
                    true,
                    RuntimeOp::FloatLog,
                ),
                method(
                    "sin",
                    Vec::new(),
                    Type::Float,
                    true,
                    RuntimeOp::FloatSin,
                ),
                method(
                    "cos",
                    Vec::new(),
                    Type::Float,
                    true,
                    RuntimeOp::FloatCos,
                ),
                method(
                    "tan",
                    Vec::new(),
                    Type::Float,
                    true,
                    RuntimeOp::FloatTan,
                ),
                method(
                    "abs",
                    Vec::new(),
                    Type::Float,
                    true,
                    RuntimeOp::FloatAbs,
                ),
            ]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::Record,
            methods: method_map(vec![
                method(
                    "has",
                    vec![param("field", Type::Str)],
                    Type::Bool,
                    true,
                    RuntimeOp::RecordHas,
                ),
                method(
                    "get",
                    vec![param("field", Type::Str)],
                    result(Type::Any),
                    true,
                    RuntimeOp::RecordGet,
                ),
                method(
                    "keys",
                    Vec::new(),
                    Type::List(Box::new(Type::Str)),
                    true,
                    RuntimeOp::RecordKeys,
                ),
            ]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::Map,
            methods: method_map(vec![
                method(
                    "len",
                    Vec::new(),
                    Type::Int,
                    true,
                    RuntimeOp::MapLen,
                ),
                method(
                    "has",
                    vec![param("key", Type::BuiltinParameter(BuiltinTypeParameter::Key))],
                    Type::Bool,
                    true,
                    RuntimeOp::MapHas,
                ),
                method(
                    "get",
                    vec![param("key", Type::BuiltinParameter(BuiltinTypeParameter::Key))],
                    result(Type::BuiltinParameter(BuiltinTypeParameter::Value)),
                    true,
                    RuntimeOp::MapGet,
                ),
                method(
                    "set",
                    vec![param("key", Type::BuiltinParameter(BuiltinTypeParameter::Key)), param("value", Type::BuiltinParameter(BuiltinTypeParameter::Value))],
                    Type::Map(Box::new(Type::BuiltinParameter(BuiltinTypeParameter::Key)), Box::new(Type::BuiltinParameter(BuiltinTypeParameter::Value))),
                    true,
                    RuntimeOp::MapSet,
                ),
                method(
                    "push",
                    vec![param("key", Type::BuiltinParameter(BuiltinTypeParameter::Key)), param("value", Type::BuiltinParameter(BuiltinTypeParameter::Element))],
                    Type::BuiltinParameter(BuiltinTypeParameter::Receiver),
                    true,
                    RuntimeOp::MapPush,
                ),
                method(
                    "remove",
                    vec![param("key", Type::BuiltinParameter(BuiltinTypeParameter::Key))],
                    Type::Map(Box::new(Type::BuiltinParameter(BuiltinTypeParameter::Key)), Box::new(Type::BuiltinParameter(BuiltinTypeParameter::Value))),
                    true,
                    RuntimeOp::MapRemove,
                ),
                method(
                    "keys",
                    Vec::new(),
                    Type::List(Box::new(Type::BuiltinParameter(BuiltinTypeParameter::Key))),
                    true,
                    RuntimeOp::MapKeys,
                ),
                method(
                    "values",
                    Vec::new(),
                    Type::List(Box::new(Type::BuiltinParameter(BuiltinTypeParameter::Value))),
                    true,
                    RuntimeOp::MapValues,
                ),
            ]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::List,
            methods: method_map(vec![
                method(
                    "len",
                    Vec::new(),
                    Type::Int,
                    true,
                    RuntimeOp::ListLen,
                ),
                method(
                    "contains",
                    vec![param("item", Type::BuiltinParameter(BuiltinTypeParameter::Element))],
                    Type::Bool,
                    true,
                    RuntimeOp::ListContains,
                ),
                method(
                    "get",
                    vec![param("index", Type::Int)],
                    result(Type::BuiltinParameter(BuiltinTypeParameter::Element)),
                    true,
                    RuntimeOp::ListGet,
                ),
                method(
                    "push",
                    vec![param("item", Type::BuiltinParameter(BuiltinTypeParameter::Element))],
                    Type::List(Box::new(Type::BuiltinParameter(BuiltinTypeParameter::Element))),
                    true,
                    RuntimeOp::ListPush,
                ),
                method(
                    "extend",
                    vec![param("other", Type::List(Box::new(Type::BuiltinParameter(BuiltinTypeParameter::Element))))],
                    Type::List(Box::new(Type::BuiltinParameter(BuiltinTypeParameter::Element))),
                    true,
                    RuntimeOp::ListExtend,
                ),
                method(
                    "join",
                    vec![default_param("separator", Type::Str)],
                    Type::Str,
                    true,
                    RuntimeOp::TextJoin,
                ),
            ]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::Stream,
            methods: method_map(vec![method(
                "collect",
                Vec::new(),
                Type::List(Box::new(Type::BuiltinParameter(BuiltinTypeParameter::Element))),
                true,
                RuntimeOp::StreamCollect,
            )]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::Str,
            methods: method_map(vec![
                method(
                    "trim",
                    Vec::new(),
                    Type::Str,
                    true,
                    RuntimeOp::TextTrim,
                ),
                method(
                    "starts_with",
                    vec![param("prefix", Type::Str)],
                    Type::Bool,
                    true,
                    RuntimeOp::TextStartsWith,
                ),
                method(
                    "ends_with",
                    vec![param("suffix", Type::Str)],
                    Type::Bool,
                    true,
                    RuntimeOp::TextEndsWith,
                ),
                method(
                    "contains",
                    vec![param("needle", Type::Str)],
                    Type::Bool,
                    true,
                    RuntimeOp::TextContains,
                ),
                method(
                    "lines",
                    Vec::new(),
                    Type::Stream(Box::new(Type::Str)),
                    true,
                    RuntimeOp::TextStreamLines,
                ),
                method(
                    "words",
                    Vec::new(),
                    Type::List(Box::new(Type::Str)),
                    true,
                    RuntimeOp::TextWords,
                ),
                method(
                    "split",
                    vec![
                        param("separator", Type::Str),
                        default_param("maxsplit", Type::Int),
                    ],
                    Type::List(Box::new(Type::Str)),
                    true,
                    RuntimeOp::TextSplit,
                ),
                method(
                    "fields",
                    vec![default_param("delimiter", Type::Str)],
                    Type::List(Box::new(Type::Str)),
                    true,
                    RuntimeOp::TextFields,
                ),
                method(
                    "replace",
                    vec![param("from", Type::Str), param("to", Type::Str)],
                    Type::Str,
                    true,
                    RuntimeOp::TextReplace,
                ),
                method(
                    "wrap",
                    vec![param("width", Type::Int)],
                    Type::List(Box::new(Type::Str)),
                    true,
                    RuntimeOp::TextWrap,
                ),
                method(
                    "translate",
                    vec![param("from", Type::Str), param("to", Type::Str)],
                    Type::Str,
                    true,
                    RuntimeOp::TextTranslate,
                ),
                method(
                    "lower",
                    Vec::new(),
                    Type::Str,
                    true,
                    RuntimeOp::TextLower,
                ),
                method(
                    "upper",
                    Vec::new(),
                    Type::Str,
                    true,
                    RuntimeOp::TextUpper,
                ),
                method(
                    "delete",
                    vec![param("chars", Type::Str)],
                    Type::Str,
                    true,
                    RuntimeOp::TextDelete,
                ),
                method(
                    "squeeze",
                    vec![default_param("chars", Type::Str)],
                    Type::Str,
                    true,
                    RuntimeOp::TextSqueeze,
                ),
                method(
                    "reverse",
                    Vec::new(),
                    Type::Str,
                    true,
                    RuntimeOp::TextReverse,
                ),
                method(
                    "count_lines",
                    Vec::new(),
                    Type::Int,
                    true,
                    RuntimeOp::TextCountLines,
                ),
                method(
                    "count_words",
                    Vec::new(),
                    Type::Int,
                    true,
                    RuntimeOp::TextCountWords,
                ),
                method(
                    "count_chars",
                    Vec::new(),
                    Type::Int,
                    true,
                    RuntimeOp::TextCountChars,
                ),
                method(
                    "byte_len",
                    Vec::new(),
                    Type::Int,
                    true,
                    RuntimeOp::TextByteLen,
                ),
                method(
                    "byte_at",
                    vec![
                        param("index", Type::Int),
                    ],
                    Type::Optional(Box::new(Type::Int)),
                    true,
                    RuntimeOp::TextByteAt,
                ),
                method(
                    "byte_slice",
                    vec![
                        param("offset", Type::Int),
                        default_param("length", Type::Int),
                    ],
                    Type::Str,
                    true,
                    RuntimeOp::TextByteSlice,
                ),
                method(
                    "find",
                    vec![
                        param("needle", Type::Str),
                        default_param("start", Type::Int),
                    ],
                    Type::Optional(Box::new(Type::Int)),
                    true,
                    RuntimeOp::TextFind,
                ),
                method(
                    "parse_int",
                    Vec::new(),
                    result(Type::Int),
                    true,
                    RuntimeOp::TextParseInt,
                ),
                method(
                    "parse_int_decimal",
                    Vec::new(),
                    result(Type::Int),
                    true,
                    RuntimeOp::TextParseIntDecimal,
                ),
                method(
                    "parse_uint",
                    Vec::new(),
                    result(Type::Int),
                    true,
                    RuntimeOp::TextParseUint,
                ),
                method(
                    "parse_uint_positive",
                    Vec::new(),
                    result(Type::Int),
                    true,
                    RuntimeOp::TextParseUintPositive,
                ),
                method(
                    "parse_float",
                    Vec::new(),
                    result(Type::Float),
                    true,
                    RuntimeOp::TextParseFloat,
                ),
                method(
                    "base64_decode",
                    Vec::new(),
                    result(Type::Bytes),
                    true,
                    RuntimeOp::BytesBase64Decode,
                ),
                method(
                    "base32_decode",
                    Vec::new(),
                    result(Type::Bytes),
                    true,
                    RuntimeOp::BytesBase32Decode,
                ),
            ]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::Bytes,
            methods: method_map(vec![
                method(
                    "len",
                    Vec::new(),
                    Type::Int,
                    true,
                    RuntimeOp::BytesLen,
                ),
                method(
                    "slice",
                    vec![
                        param("offset", Type::Int),
                        default_param("length", Type::Int),
                    ],
                    Type::Bytes,
                    true,
                    RuntimeOp::BytesSlice,
                ),
                method(
                    "dump",
                    vec![default_param("format", Type::Str)],
                    Type::Str,
                    true,
                    RuntimeOp::BytesDump,
                ),
                method(
                    "strings",
                    vec![default_param("min_len", Type::Int)],
                    Type::List(Box::new(Type::Str)),
                    true,
                    RuntimeOp::BytesStrings,
                ),
                method(
                    "base64",
                    Vec::new(),
                    Type::Str,
                    true,
                    RuntimeOp::BytesBase64,
                ),
                method(
                    "base32",
                    Vec::new(),
                    Type::Str,
                    true,
                    RuntimeOp::BytesBase32,
                ),
                method(
                    "utf8",
                    Vec::new(),
                    result(Type::Str),
                    true,
                    RuntimeOp::BytesUtf8,
                ),
                method(
                    "chunks",
                    vec![param("size", Type::Int)],
                    Type::List(Box::new(Type::Bytes)),
                    true,
                    RuntimeOp::BytesChunks,
                ),
                method(
                    "compare",
                    vec![param("other", Type::Bytes)],
                    bytes_compare_type(),
                    true,
                    RuntimeOp::BytesCompare,
                ),
                method(
                    "lines",
                    Vec::new(),
                    Type::Stream(Box::new(Type::Bytes)),
                    true,
                    RuntimeOp::BytesStreamLines,
                ),
                method(
                    "count_lines",
                    Vec::new(),
                    Type::Int,
                    true,
                    RuntimeOp::BytesCountLines,
                ),
                method(
                    "trim",
                    Vec::new(),
                    Type::Bytes,
                    true,
                    RuntimeOp::BytesTrim,
                ),
                method(
                    "starts_with",
                    vec![param("prefix", Type::Bytes)],
                    Type::Bool,
                    true,
                    RuntimeOp::BytesStartsWith,
                ),
                method(
                    "ends_with",
                    vec![param("suffix", Type::Bytes)],
                    Type::Bool,
                    true,
                    RuntimeOp::BytesEndsWith,
                ),
                method(
                    "contains",
                    vec![param("needle", Type::Bytes)],
                    Type::Bool,
                    true,
                    RuntimeOp::BytesContains,
                ),
                method(
                    "lower",
                    Vec::new(),
                    Type::Bytes,
                    true,
                    RuntimeOp::BytesLower,
                ),
                method(
                    "byte_at",
                    vec![
                        param("index", Type::Int),
                    ],
                    Type::Optional(Box::new(Type::Int)),
                    true,
                    RuntimeOp::BytesByteAt,
                ),
                method(
                    "md5",
                    Vec::new(),
                    Type::Digest,
                    true,
                    RuntimeOp::HashMd5,
                ),
                method(
                    "sha1",
                    Vec::new(),
                    Type::Digest,
                    true,
                    RuntimeOp::HashSha1,
                ),
                method(
                    "sha256",
                    Vec::new(),
                    Type::Digest,
                    true,
                    RuntimeOp::HashSha256,
                ),
                method(
                    "sha512",
                    Vec::new(),
                    Type::Digest,
                    true,
                    RuntimeOp::HashSha512,
                ),
            ]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::Status,
            methods: method_map(vec![
                method(
                    "exited",
                    Vec::new(),
                    Type::Bool,
                    true,
                    RuntimeOp::StatusExited,
                ),
                method(
                    "signaled",
                    Vec::new(),
                    Type::Bool,
                    true,
                    RuntimeOp::StatusSignaled,
                ),
                method(
                    "exited_with",
                    vec![param("code", Type::Int)],
                    Type::Bool,
                    true,
                    RuntimeOp::StatusExitedWith,
                ),
                method(
                    "exit_code",
                    Vec::new(),
                    result(Type::Int),
                    true,
                    RuntimeOp::StatusExitCode,
                ),
                method(
                    "signal_number",
                    Vec::new(),
                    result(Type::Int),
                    true,
                    RuntimeOp::StatusSignalNumber,
                ),
            ]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::ProcessHandle,
            methods: method_map(vec![method(
                "cancel",
                vec![
                    default_param("signal", Type::Str),
                    default_param("kill_after", Type::Duration),
                ],
                Type::Result(
                    Box::new(Type::Unit),
                    Box::new(Type::ProcessError),
                ),
                false,
                RuntimeOp::ProcessHandleCancel,
            )]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::FsRoot,
            methods: method_map(vec![
                method(
                    "close",
                    Vec::new(),
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsCloseRoot,
                ),
                method(
                    "host_path",
                    Vec::new(),
                    result(Type::Path),
                    false,
                    RuntimeOp::FsRootPath,
                ),
                method(
                    "open_root",
                    vec![param("path", Type::Path)],
                    result(fs_root_type()),
                    false,
                    RuntimeOp::FsRootOpenRoot,
                ),
                method(
                    "read_bytes",
                    vec![param("path", Type::Path)],
                    result(Type::Bytes),
                    false,
                    RuntimeOp::FsRootRead,
                ),
                method(
                    "read_result",
                    vec![
                        param("path", Type::Path),
                        default_param("max_bytes", Type::Int),
                        ],
                    result(fs_root_read_result_type()),
                    false,
                    RuntimeOp::FsRootReadResult,
                ),
                method(
                    "filesystem_stats",
                    vec![param("path", Type::Path)],
                    result(fs_root_filesystem_stats_type()),
                    false,
                    RuntimeOp::FsRootFilesystemStats,
                ),
                method(
                    "children",
                    vec![
                        param("path", Type::Path),
                        default_param("max_entries", Type::Int),
                        ],
                    result(fs_root_children_result_type()),
                    false,
                    RuntimeOp::FsRootChildren,
                ),
                method(
                    "read_text",
                    vec![param("path", Type::Path)],
                    result(Type::Str),
                    false,
                    RuntimeOp::FsRootReadText,
                ),
                method(
                    "write",
                    vec![
                        param("path", Type::Path),
                        param("data", Type::Bytes),
                        ],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsRootWrite,
                ),
                method(
                    "write",
                    vec![
                        param("path", Type::Path),
                        param("data", Type::Str),
                        ],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsRootWrite,
                ),
                method(
                    "write_atomic",
                    vec![
                        param("path", Type::Path),
                        param("data", Type::Bytes),
                        ],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsRootWriteAtomic,
                ),
                method(
                    "write_atomic",
                    vec![
                        param("path", Type::Path),
                        param("data", Type::Str),
                        ],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsRootWriteAtomic,
                ),
                method(
                    "metadata",
                    vec![param("path", Type::Path)],
                    result(fs_entry_type()),
                    false,
                    RuntimeOp::FsRootMetadata,
                ),
                method(
                    "exists",
                    vec![param("path", Type::Path)],
                    result(Type::Bool),
                    false,
                    RuntimeOp::FsRootExists,
                ),
                method(
                    "mkdir",
                    vec![
                        param("path", Type::Path),
                        default_param("mode", Type::Int),
                        default_param("parents", Type::Bool),
                        ],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsRootMkdir,
                ),
                method(
                    "remove",
                    vec![
                        param("path", Type::Path),
                        default_param("dir", Type::Bool),
                        ],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsRootRemove,
                ),
                method(
                    "readlink",
                    vec![param("path", Type::Path)],
                    result(Type::Path),
                    false,
                    RuntimeOp::FsRootReadlink,
                ),
                method(
                    "readlink_result",
                    vec![param("path", Type::Path)],
                    result(fs_root_readlink_result_type()),
                    false,
                    RuntimeOp::FsRootReadlinkResult,
                ),
                method(
                    "symlink",
                    vec![
                        param("target", Type::Path),
                        param("path", Type::Path),
                        default_param("parents", Type::Bool),
                        default_param("overwrite", Type::Bool),
                        ],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsRootSymlink,
                ),
                method(
                    "chmod",
                    vec![
                        param("path", Type::Path),
                        param("mode", Type::Int),
                        ],
                    result(Type::Unit),
                    false,
                    RuntimeOp::FsRootChmod,
                ),
            ]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::NetJob,
            methods: method_map(vec![
                method(
                    "wait",
                    Vec::new(),
                    result(net_response_type(true)),
                    false,
                    RuntimeOp::NetJobWait,
                ),
                method(
                    "cancel",
                    Vec::new(),
                    result(Type::Unit),
                    false,
                    RuntimeOp::NetJobCancel,
                ),
            ]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::Digest,
            methods: method_map(vec![
                method(
                    "hex",
                    Vec::new(),
                    Type::Str,
                    true,
                    RuntimeOp::DigestHex,
                ),
                method(
                    "base64",
                    Vec::new(),
                    Type::Str,
                    true,
                    RuntimeOp::DigestBase64,
                ),
            ]),
        },
        MethodReceiverSig {
            receiver: MethodReceiver::Regex,
            methods: method_map(vec![
                method(
                    "matches",
                    vec![param("text", Type::Str)],
                    Type::Bool,
                    true,
                    RuntimeOp::RegexMatches,
                ),
                method(
                    "find",
                    vec![param("text", Type::Str)],
                    Type::List(Box::new(regex_match_type())),
                    true,
                    RuntimeOp::RegexFind,
                ),
                method(
                    "captures",
                    vec![param("text", Type::Str)],
                    Type::List(Box::new(Type::Str)),
                    true,
                    RuntimeOp::RegexCaptures,
                ),
                method(
                    "replace",
                    vec![param("text", Type::Str), param("replacement", Type::Str)],
                    Type::Str,
                    true,
                    RuntimeOp::RegexReplace,
                ),
            ]),
        },
    ];
    for receiver in &mut receivers {
        for method in &mut receiver.methods {
            for overload in &mut method.overloads {
                let variable = Type::BuiltinParameter;
                overload.receiver_ty = match receiver.receiver {
                    MethodReceiver::List => Some(Type::List(Box::new(if overload.sig.op == RuntimeOp::TextJoin { Type::Str } else { variable(BuiltinTypeParameter::Element) }))),
                    MethodReceiver::Map => Some(Type::Map(Box::new(variable(BuiltinTypeParameter::Key)), Box::new(if overload.sig.op == RuntimeOp::MapPush { Type::List(Box::new(variable(BuiltinTypeParameter::Element))) } else { variable(BuiltinTypeParameter::Value) }))),
                    MethodReceiver::Stream => Some(Type::Stream(Box::new(variable(BuiltinTypeParameter::Element)))),
                    MethodReceiver::Result => Some(Type::Result(Box::new(variable(BuiltinTypeParameter::Element)), Box::new(variable(BuiltinTypeParameter::Error)))),
                    _ => None,
                };
            }
        }
    }
    receivers
}

fn method(
    name: &'static str,
    params: Vec<ParamSig>,
    return_ty: Type,
    pure: bool,
    op: RuntimeOp,
) -> (&'static str, MethodSig) {
    method_with_arg_check(name, params, return_ty, pure, op, ApiArgCheck::Standard)
}

fn method_with_arg_check(
    name: &'static str,
    params: Vec<ParamSig>,
    return_ty: Type,
    pure: bool,
    op: RuntimeOp,
    arg_check: ApiArgCheck,
) -> (&'static str, MethodSig) {
    (
        name,
        MethodSig {
            sig: sig_with_arg_check(params, return_ty, pure, op, arg_check),
            receiver_ty: None,
        },
    )
}

fn method_map(entries: Vec<(&'static str, MethodSig)>) -> Vec<NamedMethodSigs> {
    let mut methods = Vec::<NamedMethodSigs>::new();
    for (name, method) in entries {
        if let Some(entry) = methods.iter_mut().find(|entry| entry.name == name) {
            entry.overloads.push(method);
        } else {
            methods.push(NamedMethodSigs {
                name,
                overloads: vec![method],
            });
        }
    }
    methods
}

fn bytes_compare_type() -> Type {
    Type::Record(btree_map(vec![
        ("equal".to_string(), Type::Bool),
        ("byte".to_string(), Type::Int),
        ("line".to_string(), Type::Int),
        ("left".to_string(), Type::Int),
        ("right".to_string(), Type::Int),
    ]))
}

pub(in crate::signature) fn cli_token_type() -> Type {
    Type::Record(btree_map(vec![
        ("kind".to_string(), Type::Str),
        ("name".to_string(), Type::Str),
        ("value".to_string(), Type::Str),
    ]))
}

pub(in crate::signature) fn bytes_copy_type() -> Type {
    Type::Record(btree_map(vec![
        ("bytes".to_string(), Type::Int),
        ("blocks".to_string(), Type::Int),
    ]))
}
