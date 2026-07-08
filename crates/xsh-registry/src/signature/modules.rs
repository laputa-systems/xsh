#![allow(clippy::single_call_fn)]

use super::methods::{bytes_copy_type, cli_token_type};
use super::streams::fs_entry_stream;
use super::{
    ApiArgCheck, ApiSpec, BTreeMap, ModuleEntry, ModuleSig, RuntimeOp, Type, archive_entry_type,
    btree_map, default_param, diff_result_type, dns_host_type, dns_lookup_type, elf_info_type,
    env_entry_type, env_path_entry_type, fs_copy_tree_result_type, fs_entry_type,
    fs_filesystem_stats_type, fs_lock_type, fs_mount_type, fs_remove_manifest_result_type,
    fs_root_type, group_record_type, linux_blkid_type, linux_block_device_type,
    linux_disk_usage_type, linux_file_attrs_type, linux_fsck_type, linux_interface_type,
    linux_loop_device_type, linux_meminfo_type, linux_modinfo_type, linux_module_type,
    linux_open_file_type, linux_partition_table_type, linux_rfkill_type, linux_route_type,
    linux_uevent_type, measured_command_type, mime_info_type, mime_parse_type, module_sig,
    net_pool_type, net_response_type, param, patch_result_type, process_entry_type,
    process_port_type, process_stats_type, process_thread_type, process_wait_any_type, result, sig,
    sig_with_arg_check, signal_record_type, spawn_record_type, system_memory_type,
    system_os_release_type, test_call_type, test_context_type, test_script_output_type,
    uname_record_type, unix_child_event_type, unix_id_type, unix_kill_all_result_type,
    unix_logged_process_group_type, unix_pid1_event_type, unix_pid1_shutdown_type,
    unix_spawned_child_type, unix_tty_attrs_type, user_record_type, value_methods,
};

pub(in crate::signature) fn build_api_spec() -> ApiSpec {
    ApiSpec::new(
        vec![
            ModuleEntry {
                name: "applet",
                sig: applet_module(),
            },
            ModuleEntry {
                name: "archive",
                sig: archive_module(),
            },
            ModuleEntry {
                name: "bytes",
                sig: bytes_module(),
            },
            ModuleEntry {
                name: "cli",
                sig: cli_module(),
            },
            ModuleEntry {
                name: "cpu",
                sig: cpu_module(),
            },
            ModuleEntry {
                name: "diff",
                sig: diff_module(),
            },
            ModuleEntry {
                name: "dns",
                sig: dns_module(),
            },
            ModuleEntry {
                name: "elf",
                sig: elf_module(),
            },
            ModuleEntry {
                name: "env",
                sig: env_module(),
            },
            ModuleEntry {
                name: "fs",
                sig: fs_module(),
            },
            ModuleEntry {
                name: "group",
                sig: group_module(),
            },
            ModuleEntry {
                name: "hash",
                sig: hash_module(),
            },
            ModuleEntry {
                name: "ini",
                sig: ini_module(),
            },
            ModuleEntry {
                name: "io",
                sig: io_module(),
            },
            ModuleEntry {
                name: "json",
                sig: json_module(),
            },
            ModuleEntry {
                name: "linux",
                sig: linux_module(),
            },
            ModuleEntry {
                name: "map",
                sig: map_module(),
            },
            ModuleEntry {
                name: "mime",
                sig: mime_module(),
            },
            ModuleEntry {
                name: "record",
                sig: record_module(),
            },
            ModuleEntry {
                name: "module",
                sig: module_module(),
            },
            ModuleEntry {
                name: "net",
                sig: net_module(),
            },
            ModuleEntry {
                name: "patch",
                sig: patch_module(),
            },
            ModuleEntry {
                name: "path",
                sig: path_module(),
            },
            ModuleEntry {
                name: "process",
                sig: process_module(),
            },
            ModuleEntry {
                name: "system",
                sig: system_module(),
            },
            ModuleEntry {
                name: "regex",
                sig: regex_module(),
            },
            ModuleEntry {
                name: "shlex",
                sig: shlex_module(),
            },
            ModuleEntry {
                name: "set",
                sig: set_module(),
            },
            ModuleEntry {
                name: "test",
                sig: test_module(),
            },
            ModuleEntry {
                name: "time",
                sig: time_module(),
            },
            ModuleEntry {
                name: "tui",
                sig: tui_module(),
            },
            ModuleEntry {
                name: "unix",
                sig: unix_module(),
            },
            ModuleEntry {
                name: "user",
                sig: user_module(),
            },
            ModuleEntry {
                name: "utils",
                sig: utils_module(),
            },
        ],
        value_methods(),
    )
}

fn applet_user_type() -> Type {
    Type::Record(btree_map(vec![
        ("name".to_string(), Type::Str),
        ("uid".to_string(), Type::Int),
        ("gid".to_string(), Type::Int),
        ("home".to_string(), Type::Path),
        ("shell".to_string(), Type::Str),
    ]))
}

fn applet_module() -> ModuleSig {
    let argv = || vec![param("argv", Type::List(Box::new(Type::Str)))];
    let user = || param("user", applet_user_type());
    module_sig(vec![
        (
            "hash_password",
            sig(
                vec![param("password", Type::Str), param("algorithm", Type::Str)],
                result(Type::Str),
                false,
                RuntimeOp::AppletHashPassword,
            ),
        ),
        (
            "verify_password",
            sig(
                vec![param("password", Type::Str), param("hash", Type::Str)],
                Type::Bool,
                false,
                RuntimeOp::AppletVerifyPassword,
            ),
        ),
        (
            "current_euid",
            sig(Vec::new(), Type::Int, false, RuntimeOp::AppletCurrentEuid),
        ),
        (
            "current_exe",
            sig(
                Vec::new(),
                result(Type::Path),
                false,
                RuntimeOp::AppletCurrentExe,
            ),
        ),
        (
            "login_session",
            sig(
                vec![
                    user(),
                    param("preserve_env", Type::Bool),
                    param("host", Type::Str),
                ],
                result(Type::Int),
                false,
                RuntimeOp::AppletLoginSession,
            ),
        ),
        (
            "su_session",
            sig(
                vec![
                    user(),
                    param("login", Type::Bool),
                    param("preserve_env", Type::Bool),
                    param("shell", Type::Str),
                    param("command", Type::Str),
                    param("extra_args", Type::List(Box::new(Type::Str))),
                ],
                result(Type::Int),
                false,
                RuntimeOp::AppletSuSession,
            ),
        ),
        (
            "sulogin_session",
            sig(
                vec![user()],
                result(Type::Int),
                false,
                RuntimeOp::AppletSuloginSession,
            ),
        ),
        (
            "mdev",
            sig(argv(), result(Type::Int), false, RuntimeOp::AppletMdev),
        ),
    ])
}

fn archive_module() -> ModuleSig {
    module_sig(vec![
        (
            "compress",
            sig(
                vec![
                    param("source", Type::Path),
                    param("dest", Type::Path),
                    default_param("format", Type::Str),
                    default_param("level", Type::Int),
                    default_param("overwrite", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::ArchiveCompress,
            ),
        ),
        (
            "decompress",
            sig(
                vec![
                    param("source", Type::Path),
                    param("dest", Type::Path),
                    default_param("format", Type::Str),
                    default_param("overwrite", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::ArchiveDecompress,
            ),
        ),
        (
            "decompress_bytes",
            sig(
                vec![
                    param("source", Type::Path),
                    default_param("format", Type::Str),
                ],
                result(Type::Bytes),
                false,
                RuntimeOp::ArchiveDecompressBytes,
            ),
        ),
        (
            "cpio_create",
            sig(
                vec![
                    param("path", Type::Path),
                    param("root", Type::Path),
                    param("entries", Type::List(Box::new(Type::Path))),
                    default_param("overwrite", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::ArchiveCpioCreate,
            ),
        ),
        (
            "cpio_extract",
            sig(
                vec![
                    param("path", Type::Path),
                    param("dest", Type::Path),
                    default_param("overwrite", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::ArchiveCpioExtract,
            ),
        ),
        (
            "cpio_list",
            sig(
                vec![param("path", Type::Path)],
                result(Type::List(Box::new(archive_entry_type()))),
                false,
                RuntimeOp::ArchiveCpioList,
            ),
        ),
        (
            "tar_list",
            sig(
                vec![
                    param("path", Type::Path),
                    default_param("compression", Type::Str),
                    default_param("members", Type::List(Box::new(Type::Path))),
                ],
                result(Type::List(Box::new(archive_entry_type()))),
                false,
                RuntimeOp::ArchiveTarList,
            ),
        ),
        (
            "tar_extract",
            sig(
                vec![
                    param("path", Type::Path),
                    param("dest", Type::Path),
                    default_param("strip_components", Type::Int),
                    default_param("compression", Type::Str),
                    default_param("overwrite", Type::Bool),
                    default_param("members", Type::List(Box::new(Type::Path))),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::ArchiveTarExtract,
            ),
        ),
        (
            "tar_create",
            sig(
                vec![
                    param("path", Type::Path),
                    param("root", Type::Path),
                    param("entries", Type::List(Box::new(Type::Path))),
                    default_param("compression", Type::Str),
                    default_param("overwrite", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::ArchiveTarCreate,
            ),
        ),
        (
            "zip_extract",
            sig(
                vec![
                    param("path", Type::Path),
                    param("dest", Type::Path),
                    default_param("overwrite", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::ArchiveZipExtract,
            ),
        ),
        (
            "zip_list",
            sig(
                vec![param("path", Type::Path)],
                result(Type::List(Box::new(archive_entry_type()))),
                false,
                RuntimeOp::ArchiveZipList,
            ),
        ),
    ])
}

fn elf_module() -> ModuleSig {
    module_sig(vec![(
        "inspect",
        sig(
            vec![param("path", Type::Path)],
            result(elf_info_type()),
            false,
            RuntimeOp::ElfInspect,
        ),
    )])
}

fn cli_module() -> ModuleSig {
    module_sig(vec![
        (
            "parse",
            sig(
                vec![
                    param("argv", Type::List(Box::new(Type::Str))),
                    param("schema", Type::Record(BTreeMap::new())),
                    default_param("command", Type::Str),
                ],
                result(Type::Record(BTreeMap::new())),
                true,
                RuntimeOp::CliParse,
            ),
        ),
        (
            "parse_full",
            sig(
                vec![
                    param("argv", Type::List(Box::new(Type::Str))),
                    param("schema", Type::Record(BTreeMap::new())),
                    default_param("env", Type::Record(BTreeMap::new())),
                    default_param("command", Type::Str),
                ],
                result(Type::Record(BTreeMap::new())),
                true,
                RuntimeOp::CliParseFull,
            ),
        ),
        (
            "usage",
            sig(
                vec![
                    param("schema", Type::Record(BTreeMap::new())),
                    default_param("command", Type::Str),
                ],
                Type::Str,
                true,
                RuntimeOp::CliUsage,
            ),
        ),
        (
            "commands",
            sig(
                vec![
                    param("argv", Type::List(Box::new(Type::Str))),
                    param("commands", Type::Record(BTreeMap::new())),
                ],
                result(Type::Record(BTreeMap::new())),
                true,
                RuntimeOp::CliCommands,
            ),
        ),
        (
            "commands",
            sig(
                vec![
                    param("argv", Type::List(Box::new(Type::Str))),
                    param("rootless_default", Type::Str),
                    param("commands", Type::Record(BTreeMap::new())),
                    default_param("fallback_command", Type::Record(BTreeMap::new())),
                ],
                result(Type::Record(BTreeMap::new())),
                true,
                RuntimeOp::CliCommands,
            ),
        ),
        (
            "tokens",
            sig(
                vec![
                    param("argv", Type::List(Box::new(Type::Str))),
                    default_param("value_flags", Type::List(Box::new(Type::Str))),
                ],
                result(Type::List(Box::new(cli_token_type()))),
                true,
                RuntimeOp::CliTokens,
            ),
        ),
    ])
}

fn io_module() -> ModuleSig {
    module_sig(vec![
        (
            "stdin_bytes",
            sig(
                Vec::new(),
                result(Type::Bytes),
                false,
                RuntimeOp::IoStdinBytes,
            ),
        ),
        (
            "stdin_text",
            sig(Vec::new(), result(Type::Str), false, RuntimeOp::IoStdinText),
        ),
        (
            "stdin_line",
            sig(Vec::new(), result(Type::Str), false, RuntimeOp::IoStdinLine),
        ),
        (
            "write_stdout",
            sig(
                vec![param("text", Type::Str)],
                result(Type::Unit),
                false,
                RuntimeOp::IoWriteStdout,
            ),
        ),
        (
            "write_stdout_bytes",
            sig(
                vec![param("data", Type::Bytes)],
                result(Type::Unit),
                false,
                RuntimeOp::IoWriteStdoutBytes,
            ),
        ),
    ])
}

fn ini_module() -> ModuleSig {
    let record = || Type::Record(BTreeMap::new());
    module_sig(vec![
        (
            "decode",
            sig(
                vec![param("text", Type::Str)],
                result(record()),
                true,
                RuntimeOp::IniDecode,
            ),
        ),
        (
            "read",
            sig(
                vec![param("path", Type::Path)],
                result(record()),
                false,
                RuntimeOp::IniRead,
            ),
        ),
        (
            "encode",
            sig(
                vec![param("value", record())],
                result(Type::Str),
                true,
                RuntimeOp::IniEncode,
            ),
        ),
        (
            "write",
            sig(
                vec![
                    param("path", Type::Path),
                    param("value", record()),
                    default_param("overwrite", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::IniWrite,
            ),
        ),
    ])
}

fn bytes_module() -> ModuleSig {
    module_sig(vec![
        (
            "zero",
            sig(
                vec![param("length", Type::Int)],
                result(Type::Bytes),
                true,
                RuntimeOp::BytesZero,
            ),
        ),
        (
            "from_ints",
            sig(
                vec![param("values", Type::List(Box::new(Type::Int)))],
                result(Type::Bytes),
                true,
                RuntimeOp::BytesFromInts,
            ),
        ),
        (
            "from_text",
            sig(
                vec![param("text", Type::Str)],
                Type::Bytes,
                true,
                RuntimeOp::BytesFromText,
            ),
        ),
        (
            "concat",
            sig(
                vec![param("chunks", Type::List(Box::new(Type::Bytes)))],
                Type::Bytes,
                true,
                RuntimeOp::BytesConcat,
            ),
        ),
        (
            "human",
            sig(
                vec![param("size", Type::Int)],
                Type::Str,
                true,
                RuntimeOp::BytesHuman,
            ),
        ),
        (
            "pack_le",
            sig(
                vec![param("value", Type::Int), param("width", Type::Int)],
                result(Type::Bytes),
                true,
                RuntimeOp::BytesPackLe,
            ),
        ),
        (
            "pack_be",
            sig(
                vec![param("value", Type::Int), param("width", Type::Int)],
                result(Type::Bytes),
                true,
                RuntimeOp::BytesPackBe,
            ),
        ),
        (
            "unpack_le",
            sig(
                vec![
                    param("data", Type::Bytes),
                    param("width", Type::Int),
                    default_param("offset", Type::Int),
                ],
                result(Type::Int),
                true,
                RuntimeOp::BytesUnpackLe,
            ),
        ),
        (
            "unpack_be",
            sig(
                vec![
                    param("data", Type::Bytes),
                    param("width", Type::Int),
                    default_param("offset", Type::Int),
                ],
                result(Type::Int),
                true,
                RuntimeOp::BytesUnpackBe,
            ),
        ),
        (
            "read_at",
            sig(
                vec![
                    param("path", Type::Path),
                    param("offset", Type::Int),
                    param("length", Type::Int),
                ],
                result(Type::Bytes),
                false,
                RuntimeOp::BytesReadAt,
            ),
        ),
        (
            "write_at",
            sig(
                vec![
                    param("path", Type::Path),
                    param("offset", Type::Int),
                    param("data", Type::Bytes),
                    default_param("create", Type::Bool),
                ],
                result(Type::Int),
                false,
                RuntimeOp::BytesWriteAt,
            ),
        ),
        (
            "zero_at",
            sig(
                vec![
                    param("path", Type::Path),
                    param("offset", Type::Int),
                    param("length", Type::Int),
                    default_param("create", Type::Bool),
                ],
                result(Type::Int),
                false,
                RuntimeOp::BytesZeroAt,
            ),
        ),
        (
            "copy",
            sig(
                vec![
                    param("source", Type::Path),
                    param("dest", Type::Path),
                    default_param("block_size", Type::Int),
                    default_param("count", Type::Int),
                    default_param("skip", Type::Int),
                    default_param("seek", Type::Int),
                    default_param("overwrite", Type::Bool),
                ],
                result(bytes_copy_type()),
                false,
                RuntimeOp::BytesCopy,
            ),
        ),
        (
            "copy_file",
            sig(
                vec![
                    param("source", Type::Path),
                    param("dest", Type::Path),
                    default_param("source_offset", Type::Int),
                    default_param("dest_offset", Type::Int),
                    default_param("length", Type::Int),
                    default_param("create", Type::Bool),
                    default_param("truncate", Type::Bool),
                ],
                result(bytes_copy_type()),
                false,
                RuntimeOp::BytesCopyFile,
            ),
        ),
    ])
}

fn cpu_module() -> ModuleSig {
    module_sig(vec![(
        "count",
        sig(Vec::new(), Type::Int, true, RuntimeOp::CpuCount),
    )])
}

fn dns_module() -> ModuleSig {
    module_sig(vec![
        (
            "lookup",
            sig(
                vec![
                    param("name", Type::Str),
                    default_param("record", Type::Str),
                    default_param("server", Type::Str),
                    default_param("timeout", Type::Duration),
                ],
                result(Type::List(Box::new(dns_lookup_type()))),
                false,
                RuntimeOp::DnsLookup,
            ),
        ),
        (
            "resolve_host",
            sig(
                vec![param("name", Type::Str), default_param("family", Type::Str)],
                result(Type::List(Box::new(dns_host_type()))),
                false,
                RuntimeOp::DnsResolveHost,
            ),
        ),
        (
            "reverse",
            sig(
                vec![param("addr", Type::Str)],
                result(Type::List(Box::new(Type::Str))),
                false,
                RuntimeOp::DnsReverse,
            ),
        ),
        (
            "nameservers",
            sig(
                Vec::new(),
                result(Type::List(Box::new(Type::Str))),
                false,
                RuntimeOp::DnsNameservers,
            ),
        ),
    ])
}

fn diff_module() -> ModuleSig {
    module_sig(vec![(
        "unified",
        sig(
            vec![
                param("original", Type::Path),
                param("modified", Type::Path),
                default_param("context", Type::Int),
            ],
            result(diff_result_type()),
            false,
            RuntimeOp::DiffUnified,
        ),
    )])
}

fn patch_module() -> ModuleSig {
    module_sig(vec![(
        "apply",
        sig(
            vec![
                param("root", Type::Path),
                param("text", Type::Str),
                default_param("strip_components", Type::Int),
                default_param("overwrite", Type::Bool),
            ],
            result(patch_result_type()),
            false,
            RuntimeOp::PatchApply,
        ),
    )])
}

fn map_module() -> ModuleSig {
    let map_unknown = || Type::Map(Box::new(Type::Any));
    module_sig(vec![(
        "empty",
        sig(Vec::new(), map_unknown(), true, RuntimeOp::MapEmpty),
    )])
}

fn set_module() -> ModuleSig {
    let set_type = || Type::Map(Box::new(Type::Bool));
    module_sig(vec![
        (
            "empty",
            sig(Vec::new(), set_type(), true, RuntimeOp::SetEmpty),
        ),
        (
            "from",
            sig(
                vec![param("items", Type::List(Box::new(Type::Str)))],
                set_type(),
                true,
                RuntimeOp::SetFrom,
            ),
        ),
        (
            "has",
            sig(
                vec![param("set", set_type()), param("item", Type::Str)],
                Type::Bool,
                true,
                RuntimeOp::SetHas,
            ),
        ),
        (
            "add",
            sig(
                vec![param("set", set_type()), param("item", Type::Str)],
                set_type(),
                true,
                RuntimeOp::SetAdd,
            ),
        ),
        (
            "remove",
            sig(
                vec![param("set", set_type()), param("item", Type::Str)],
                set_type(),
                true,
                RuntimeOp::SetRemove,
            ),
        ),
    ])
}

fn mime_module() -> ModuleSig {
    module_sig(vec![
        (
            "lookup_ext",
            sig(
                vec![param("ext", Type::Str)],
                Type::Optional(Box::new(mime_info_type())),
                false,
                RuntimeOp::MimeLookupExt,
            ),
        ),
        (
            "lookup_path",
            sig(
                vec![param("path", Type::Path)],
                Type::Optional(Box::new(mime_info_type())),
                false,
                RuntimeOp::MimeLookupPath,
            ),
        ),
        (
            "parse",
            sig(
                vec![param("value", Type::Str)],
                result(mime_parse_type()),
                true,
                RuntimeOp::MimeParse,
            ),
        ),
    ])
}

fn record_module() -> ModuleSig {
    let record_unknown = || Type::Record(BTreeMap::new());
    module_sig(vec![(
        "require",
        sig(
            vec![
                param("record", record_unknown()),
                param("required", record_unknown()),
                default_param("optional", record_unknown()),
                default_param("source", Type::Path),
            ],
            result(record_unknown()),
            false,
            RuntimeOp::RecordRequire,
        ),
    )])
}

fn regex_module() -> ModuleSig {
    module_sig(vec![(
        "compile",
        sig(
            vec![param("pattern", Type::Str)],
            result(Type::Regex),
            true,
            RuntimeOp::RegexCompile,
        ),
    )])
}

fn shlex_module() -> ModuleSig {
    module_sig(vec![
        (
            "quote",
            sig(
                vec![param("value", Type::Str)],
                Type::Str,
                true,
                RuntimeOp::ShlexQuote,
            ),
        ),
        (
            "join",
            sig(
                vec![param("argv", Type::List(Box::new(Type::Str)))],
                Type::Str,
                true,
                RuntimeOp::ShlexJoin,
            ),
        ),
    ])
}

fn module_module() -> ModuleSig {
    module_sig(vec![(
        "load",
        sig(
            vec![param("path", Type::Path)],
            result(Type::Module(BTreeMap::new())),
            false,
            RuntimeOp::ModuleLoad,
        ),
    )])
}

fn net_module() -> ModuleSig {
    module_sig(vec![
        (
            "request",
            sig(
                vec![param("request", Type::Record(BTreeMap::new()))],
                result(net_response_type(true)),
                false,
                RuntimeOp::NetRequest,
            ),
        ),
        (
            "download",
            sig(
                vec![param("request", Type::Record(BTreeMap::new()))],
                result(net_response_type(false)),
                false,
                RuntimeOp::NetDownload,
            ),
        ),
        (
            "upload",
            sig(
                vec![param("request", Type::Record(BTreeMap::new()))],
                result(net_response_type(false)),
                false,
                RuntimeOp::NetUpload,
            ),
        ),
        (
            "pool",
            sig(
                vec![
                    default_param("name", Type::Str),
                    default_param("max_idle_per_host", Type::Int),
                    default_param("idle_timeout", Type::Duration),
                ],
                result(net_pool_type()),
                false,
                RuntimeOp::NetPool,
            ),
        ),
        (
            "close_pool",
            sig(
                vec![default_param("name", Type::Str)],
                result(Type::Unit),
                false,
                RuntimeOp::NetClosePool,
            ),
        ),
        (
            "close_all_pools",
            sig(
                Vec::new(),
                result(Type::Unit),
                false,
                RuntimeOp::NetCloseAllPools,
            ),
        ),
    ])
}

fn env_module() -> ModuleSig {
    module_sig(vec![
        (
            "get",
            sig(
                vec![param("name", Type::Str)],
                result(Type::Str),
                false,
                RuntimeOp::EnvGet,
            ),
        ),
        (
            "get_or",
            sig(
                vec![
                    param("name", Type::Str),
                    default_param("fallback", Type::Str),
                ],
                result(Type::Str),
                false,
                RuntimeOp::EnvGetOr,
            ),
        ),
        (
            "bool",
            sig(
                vec![
                    param("name", Type::Str),
                    default_param("fallback", Type::Bool),
                ],
                result(Type::Bool),
                false,
                RuntimeOp::EnvBool,
            ),
        ),
        (
            "path",
            sig(
                vec![
                    param("name", Type::Str),
                    default_param("fallback", Type::Path),
                ],
                result(Type::Path),
                false,
                RuntimeOp::EnvPath,
            ),
        ),
        (
            "int",
            sig(
                vec![
                    param("name", Type::Str),
                    default_param("fallback", Type::Int),
                ],
                result(Type::Int),
                false,
                RuntimeOp::EnvInt,
            ),
        ),
        (
            "list",
            sig(
                Vec::new(),
                result(Type::List(Box::new(env_entry_type()))),
                false,
                RuntimeOp::EnvList,
            ),
        ),
        (
            "path_list",
            sig(
                vec![param("name", Type::Str)],
                result(Type::List(Box::new(Type::Path))),
                false,
                RuntimeOp::EnvPathList,
            ),
        ),
        (
            "path_entries",
            sig(
                vec![param("name", Type::Str)],
                result(Type::List(Box::new(env_path_entry_type()))),
                false,
                RuntimeOp::EnvPathEntries,
            ),
        ),
    ])
}

fn fs_module() -> ModuleSig {
    module_sig(vec![
        (
            "walk",
            sig(
                vec![
                    param("path", Type::Path),
                    default_param("gitignore", Type::Bool),
                    default_param("stat", Type::Bool),
                    default_param("hidden", Type::Bool),
                ],
                result(fs_entry_stream()),
                false,
                RuntimeOp::FsWalk,
            ),
        ),
        (
            "files",
            sig(
                vec![
                    param("path", Type::Path),
                    default_param("gitignore", Type::Bool),
                    default_param("stat", Type::Bool),
                    default_param("exts", Type::List(Box::new(Type::Str))),
                    default_param("hidden", Type::Bool),
                ],
                result(fs_entry_stream()),
                false,
                RuntimeOp::FsFiles,
            ),
        ),
        (
            "dirs",
            sig(
                vec![
                    param("path", Type::Path),
                    default_param("gitignore", Type::Bool),
                    default_param("stat", Type::Bool),
                    default_param("hidden", Type::Bool),
                ],
                result(fs_entry_stream()),
                false,
                RuntimeOp::FsDirs,
            ),
        ),
        (
            "ls",
            sig(
                vec![
                    param("path", Type::Path),
                    default_param("stat", Type::Bool),
                    default_param("ordered", Type::Bool),
                ],
                result(fs_entry_stream()),
                false,
                RuntimeOp::FsLs,
            ),
        ),
        (
            "children",
            sig(
                vec![
                    param("path", Type::Path),
                    default_param("stat", Type::Bool),
                    default_param("ordered", Type::Bool),
                ],
                result(fs_entry_stream()),
                false,
                RuntimeOp::FsChildren,
            ),
        ),
        (
            "metadata",
            sig(
                vec![param("path", Type::Path)],
                result(fs_entry_type()),
                false,
                RuntimeOp::FsMetadata,
            ),
        ),
        (
            "filesystem_stats",
            sig(
                vec![param("path", Type::Path)],
                result(fs_filesystem_stats_type()),
                false,
                RuntimeOp::FsFilesystemStats,
            ),
        ),
        (
            "mounts",
            sig(
                Vec::new(),
                result(Type::List(Box::new(fs_mount_type()))),
                false,
                RuntimeOp::FsMounts,
            ),
        ),
        (
            "mount_for",
            sig(
                vec![param("path", Type::Path)],
                result(fs_mount_type()),
                false,
                RuntimeOp::FsMountFor,
            ),
        ),
        (
            "cwd",
            sig(Vec::new(), result(Type::Path), false, RuntimeOp::FsCwd),
        ),
        (
            "read_text",
            sig(
                vec![param("path", Type::Path)],
                result(Type::Str),
                false,
                RuntimeOp::FsReadText,
            ),
        ),
        (
            "write",
            sig(
                vec![param("path", Type::Path), param("data", Type::Bytes)],
                result(Type::Unit),
                false,
                RuntimeOp::FsWrite,
            ),
        ),
        (
            "write",
            sig(
                vec![param("path", Type::Path), param("data", Type::Str)],
                result(Type::Unit),
                false,
                RuntimeOp::FsWrite,
            ),
        ),
        (
            "write_atomic",
            sig(
                vec![param("path", Type::Path), param("data", Type::Bytes)],
                result(Type::Unit),
                false,
                RuntimeOp::FsWriteAtomic,
            ),
        ),
        (
            "write_atomic",
            sig(
                vec![param("path", Type::Path), param("data", Type::Str)],
                result(Type::Unit),
                false,
                RuntimeOp::FsWriteAtomic,
            ),
        ),
        (
            "exists",
            sig(
                vec![param("path", Type::Path)],
                result(Type::Bool),
                false,
                RuntimeOp::FsExists,
            ),
        ),
        (
            "executable",
            sig(
                vec![param("path", Type::Path)],
                result(Type::Bool),
                false,
                RuntimeOp::FsExecutable,
            ),
        ),
        (
            "executable",
            sig(
                vec![param("mode", Type::Int)],
                Type::Bool,
                true,
                RuntimeOp::FsExecutable,
            ),
        ),
        (
            "world_writable",
            sig(
                vec![param("mode", Type::Int)],
                Type::Bool,
                true,
                RuntimeOp::FsWorldWritable,
            ),
        ),
        (
            "sticky",
            sig(
                vec![param("mode", Type::Int)],
                Type::Bool,
                true,
                RuntimeOp::FsSticky,
            ),
        ),
        (
            "setuid",
            sig(
                vec![param("mode", Type::Int)],
                Type::Bool,
                true,
                RuntimeOp::FsSetuid,
            ),
        ),
        (
            "setgid",
            sig(
                vec![param("mode", Type::Int)],
                Type::Bool,
                true,
                RuntimeOp::FsSetgid,
            ),
        ),
        (
            "owner_executable",
            sig(
                vec![param("mode", Type::Int)],
                Type::Bool,
                true,
                RuntimeOp::FsOwnerExecutable,
            ),
        ),
        (
            "group_executable",
            sig(
                vec![param("mode", Type::Int)],
                Type::Bool,
                true,
                RuntimeOp::FsGroupExecutable,
            ),
        ),
        (
            "other_executable",
            sig(
                vec![param("mode", Type::Int)],
                Type::Bool,
                true,
                RuntimeOp::FsOtherExecutable,
            ),
        ),
        (
            "open_root",
            sig(
                vec![param("path", Type::Path)],
                result(fs_root_type()),
                false,
                RuntimeOp::FsOpenRoot,
            ),
        ),
        (
            "close_root",
            sig(
                vec![param("root", fs_root_type())],
                result(Type::Unit),
                false,
                RuntimeOp::FsCloseRoot,
            ),
        ),
        (
            "root_path",
            sig(
                vec![param("root", fs_root_type())],
                result(Type::Path),
                false,
                RuntimeOp::FsRootPath,
            ),
        ),
        (
            "root",
            sig(
                vec![param("root", fs_root_type()), param("path", Type::Path)],
                result(fs_root_type()),
                false,
                RuntimeOp::FsRootOpenRoot,
            ),
        ),
        (
            "root_read",
            sig(
                vec![param("root", fs_root_type()), param("path", Type::Path)],
                result(Type::Bytes),
                false,
                RuntimeOp::FsRootRead,
            ),
        ),
        (
            "root_read_text",
            sig(
                vec![param("root", fs_root_type()), param("path", Type::Path)],
                result(Type::Str),
                false,
                RuntimeOp::FsRootReadText,
            ),
        ),
        (
            "root_write",
            sig(
                vec![
                    param("root", fs_root_type()),
                    param("path", Type::Path),
                    param("data", Type::Bytes),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsRootWrite,
            ),
        ),
        (
            "root_write",
            sig(
                vec![
                    param("root", fs_root_type()),
                    param("path", Type::Path),
                    param("data", Type::Str),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsRootWrite,
            ),
        ),
        (
            "root_write_atomic",
            sig(
                vec![
                    param("root", fs_root_type()),
                    param("path", Type::Path),
                    param("data", Type::Bytes),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsRootWriteAtomic,
            ),
        ),
        (
            "root_write_atomic",
            sig(
                vec![
                    param("root", fs_root_type()),
                    param("path", Type::Path),
                    param("data", Type::Str),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsRootWriteAtomic,
            ),
        ),
        (
            "root_metadata",
            sig(
                vec![param("root", fs_root_type()), param("path", Type::Path)],
                result(fs_entry_type()),
                false,
                RuntimeOp::FsRootMetadata,
            ),
        ),
        (
            "root_exists",
            sig(
                vec![param("root", fs_root_type()), param("path", Type::Path)],
                result(Type::Bool),
                false,
                RuntimeOp::FsRootExists,
            ),
        ),
        (
            "root_mkdir",
            sig(
                vec![
                    param("root", fs_root_type()),
                    param("path", Type::Path),
                    default_param("mode", Type::Int),
                    default_param("parents", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsRootMkdir,
            ),
        ),
        (
            "root_remove",
            sig(
                vec![
                    param("root", fs_root_type()),
                    param("path", Type::Path),
                    default_param("dir", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsRootRemove,
            ),
        ),
        (
            "root_readlink",
            sig(
                vec![param("root", fs_root_type()), param("path", Type::Path)],
                result(Type::Path),
                false,
                RuntimeOp::FsRootReadlink,
            ),
        ),
        (
            "root_symlink",
            sig(
                vec![
                    param("root", fs_root_type()),
                    param("target", Type::Path),
                    param("path", Type::Path),
                    default_param("parents", Type::Bool),
                    default_param("overwrite", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsRootSymlink,
            ),
        ),
        (
            "root_chmod",
            sig(
                vec![
                    param("root", fs_root_type()),
                    param("path", Type::Path),
                    param("mode", Type::Int),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsRootChmod,
            ),
        ),
        (
            "root_install_file",
            sig(
                vec![
                    param("source_root", fs_root_type()),
                    param("source", Type::Path),
                    param("dest_root", fs_root_type()),
                    param("dest", Type::Path),
                    param("mode", Type::Int),
                    default_param("parents", Type::Bool),
                    default_param("overwrite", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsRootInstallFile,
            ),
        ),
        (
            "copy",
            sig(
                vec![
                    param("source", Type::Path),
                    param("dest", Type::Path),
                    default_param("overwrite", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsCopy,
            ),
        ),
        (
            "copy_tree",
            sig(
                vec![
                    param("source", Type::Path),
                    param("dest", Type::Path),
                    default_param("parents", Type::Bool),
                    default_param("overwrite", Type::Bool),
                    default_param("follow_symlinks", Type::Bool),
                ],
                result(fs_copy_tree_result_type()),
                false,
                RuntimeOp::FsCopyTree,
            ),
        ),
        (
            "rename",
            sig(
                vec![
                    param("source", Type::Path),
                    param("dest", Type::Path),
                    default_param("overwrite", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsRename,
            ),
        ),
        (
            "mkdir",
            sig(
                vec![
                    param("path", Type::Path),
                    default_param("parents", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsMkdir,
            ),
        ),
        (
            "remove",
            sig(
                vec![
                    param("path", Type::Path),
                    default_param("missing_ok", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsRemove,
            ),
        ),
        (
            "remove_manifest",
            sig(
                vec![
                    param("root", Type::Path),
                    param("manifest", Type::List(Box::new(Type::Path))),
                    default_param("missing_ok", Type::Bool),
                    default_param("prune_dirs", Type::Bool),
                ],
                result(fs_remove_manifest_result_type()),
                false,
                RuntimeOp::FsRemoveManifest,
            ),
        ),
        (
            "install",
            sig(
                vec![
                    param("source", Type::Path),
                    param("dest", Type::Path),
                    param("mode", Type::Int),
                    default_param("parents", Type::Bool),
                    default_param("overwrite", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsInstall,
            ),
        ),
        (
            "install_as",
            sig(
                vec![
                    param("source", Type::Path),
                    param("dest", Type::Path),
                    param("mode", Type::Int),
                    param("owner", user_record_type()),
                    param("group", group_record_type()),
                    default_param("parents", Type::Bool),
                    default_param("overwrite", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsInstallAs,
            ),
        ),
        (
            "chmod",
            sig(
                vec![param("path", Type::Path), param("mode", Type::Int)],
                result(Type::Unit),
                false,
                RuntimeOp::FsChmod,
            ),
        ),
        (
            "chown",
            sig(
                vec![
                    param("path", Type::Path),
                    param("owner", user_record_type()),
                    default_param("follow_symlinks", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsChown,
            ),
        ),
        (
            "chgrp",
            sig(
                vec![
                    param("path", Type::Path),
                    param("group", group_record_type()),
                    default_param("follow_symlinks", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::FsChgrp,
            ),
        ),
        (
            "mkfifo",
            sig(
                vec![param("path", Type::Path), param("mode", Type::Int)],
                result(Type::Unit),
                false,
                RuntimeOp::FsMkfifo,
            ),
        ),
        (
            "fsync",
            sig(
                vec![param("path", Type::Path)],
                result(Type::Unit),
                false,
                RuntimeOp::FsFsync,
            ),
        ),
        (
            "gitroot",
            sig(Vec::new(), result(Type::Path), false, RuntimeOp::FsGitroot),
        ),
        (
            "sync",
            sig(Vec::new(), result(Type::Unit), false, RuntimeOp::FsSync),
        ),
        (
            "symlink",
            sig(
                vec![param("target", Type::Path), param("path", Type::Path)],
                result(Type::Unit),
                false,
                RuntimeOp::FsSymlink,
            ),
        ),
        (
            "lock",
            sig(
                vec![
                    param("path", Type::Path),
                    default_param("shared", Type::Bool),
                    default_param("nonblocking", Type::Bool),
                ],
                result(fs_lock_type()),
                false,
                RuntimeOp::FsLock,
            ),
        ),
        (
            "unlock",
            sig(
                vec![param("lock", fs_lock_type())],
                result(Type::Unit),
                false,
                RuntimeOp::FsUnlock,
            ),
        ),
        (
            "tempfile",
            sig(
                Vec::new(),
                result(Type::Record(btree_map(vec![
                    ("root".to_string(), fs_root_type()),
                    ("path".to_string(), Type::Path),
                ]))),
                false,
                RuntimeOp::FsTempFile,
            ),
        ),
        (
            "tempdir",
            sig(
                Vec::new(),
                result(fs_root_type()),
                false,
                RuntimeOp::FsTempDir,
            ),
        ),
        (
            "project_root",
            sig(
                vec![
                    param("kind", Type::Str),
                    param("qualifier", Type::Str),
                    param("organization", Type::Str),
                    param("application", Type::Str),
                ],
                result(fs_root_type()),
                false,
                RuntimeOp::FsProjectRoot,
            ),
        ),
        (
            "user_root",
            sig(
                vec![param("kind", Type::Str)],
                result(fs_root_type()),
                false,
                RuntimeOp::FsUserRoot,
            ),
        ),
    ])
}

fn group_module() -> ModuleSig {
    module_sig(vec![
        (
            "current",
            sig(
                Vec::new(),
                result(group_record_type()),
                false,
                RuntimeOp::GroupCurrent,
            ),
        ),
        (
            "lookup",
            sig(
                vec![param("name", Type::Str)],
                result(group_record_type()),
                false,
                RuntimeOp::GroupLookup,
            ),
        ),
        (
            "by_gid",
            sig(
                vec![param("gid", Type::Int)],
                result(group_record_type()),
                false,
                RuntimeOp::GroupByGid,
            ),
        ),
        (
            "add",
            sig(
                vec![param("name", Type::Str), default_param("gid", Type::Int)],
                result(group_record_type()),
                false,
                RuntimeOp::GroupAdd,
            ),
        ),
        (
            "remove",
            sig(
                vec![param("name", Type::Str)],
                result(Type::Unit),
                false,
                RuntimeOp::GroupRemove,
            ),
        ),
    ])
}

fn hash_module() -> ModuleSig {
    module_sig(vec![
        (
            "md5",
            sig(
                vec![param("data", Type::Bytes)],
                Type::Digest,
                true,
                RuntimeOp::HashMd5,
            ),
        ),
        (
            "md5",
            sig(
                vec![param("path", Type::Path)],
                result(Type::Digest),
                false,
                RuntimeOp::HashMd5,
            ),
        ),
        (
            "sha1",
            sig(
                vec![param("data", Type::Bytes)],
                Type::Digest,
                true,
                RuntimeOp::HashSha1,
            ),
        ),
        (
            "sha1",
            sig(
                vec![param("path", Type::Path)],
                result(Type::Digest),
                false,
                RuntimeOp::HashSha1,
            ),
        ),
        (
            "sha256",
            sig(
                vec![param("data", Type::Bytes)],
                Type::Digest,
                true,
                RuntimeOp::HashSha256,
            ),
        ),
        (
            "sha256",
            sig(
                vec![param("path", Type::Path)],
                result(Type::Digest),
                false,
                RuntimeOp::HashSha256,
            ),
        ),
        (
            "sha512",
            sig(
                vec![param("data", Type::Bytes)],
                Type::Digest,
                true,
                RuntimeOp::HashSha512,
            ),
        ),
        (
            "sha512",
            sig(
                vec![param("path", Type::Path)],
                result(Type::Digest),
                false,
                RuntimeOp::HashSha512,
            ),
        ),
        (
            "crc32",
            sig(
                vec![param("data", Type::Bytes)],
                Type::Int,
                true,
                RuntimeOp::HashCrc32,
            ),
        ),
        (
            "crc32c",
            sig(
                vec![param("data", Type::Bytes)],
                Type::Int,
                true,
                RuntimeOp::HashCrc32c,
            ),
        ),
        (
            "parse_check_line",
            sig(
                vec![param("line", Type::Str)],
                result(Type::Record(Default::default())),
                true,
                RuntimeOp::HashParseCheckLine,
            ),
        ),
        (
            "verify_file",
            sig_with_arg_check(
                vec![
                    param("path", Type::Path),
                    default_param("sha256", Type::Str),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::HashVerifyFile,
                ApiArgCheck::HashVerifyFile,
            ),
        ),
    ])
}

fn json_module() -> ModuleSig {
    module_sig(vec![
        (
            "decode",
            sig(
                vec![param("s", Type::Str)],
                result(Type::Any),
                true,
                RuntimeOp::JsonDecode,
            ),
        ),
        (
            "encode",
            sig_with_arg_check(
                vec![
                    param("value", Type::Any),
                    default_param("pretty", Type::Bool),
                ],
                result(Type::Str),
                true,
                RuntimeOp::JsonEncode,
                ApiArgCheck::JsonCompatible,
            ),
        ),
        (
            "encode_lines",
            sig_with_arg_check(
                vec![param("values", Type::List(Box::new(Type::Any)))],
                result(Type::Str),
                true,
                RuntimeOp::JsonEncodeLines,
                ApiArgCheck::JsonCompatible,
            ),
        ),
        (
            "get",
            sig(
                vec![
                    param("value", Type::Any),
                    param("path", Type::List(Box::new(Type::Any))),
                ],
                result(Type::Any),
                true,
                RuntimeOp::JsonGet,
            ),
        ),
        (
            "get",
            sig(
                vec![
                    param("value", Type::Any),
                    param("path", Type::List(Box::new(Type::Any))),
                    param("fallback", Type::Any),
                ],
                Type::Any,
                true,
                RuntimeOp::JsonGet,
            ),
        ),
        (
            "read",
            sig(
                vec![param("path", Type::Path)],
                result(Type::Any),
                false,
                RuntimeOp::JsonRead,
            ),
        ),
        (
            "remove",
            sig(
                vec![
                    param("value", Type::Any),
                    param("path", Type::List(Box::new(Type::Any))),
                ],
                result(Type::Any),
                true,
                RuntimeOp::JsonRemove,
            ),
        ),
        (
            "set",
            sig_with_arg_check(
                vec![
                    param("value", Type::Any),
                    param("path", Type::List(Box::new(Type::Any))),
                    param("replacement", Type::Any),
                ],
                result(Type::Any),
                true,
                RuntimeOp::JsonSet,
                ApiArgCheck::JsonCompatible,
            ),
        ),
        (
            "write",
            sig_with_arg_check(
                vec![
                    param("path", Type::Path),
                    param("value", Type::Any),
                    default_param("pretty", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::JsonWrite,
                ApiArgCheck::JsonCompatible,
            ),
        ),
        (
            "write_lines",
            sig_with_arg_check(
                vec![
                    param("path", Type::Path),
                    param("values", Type::List(Box::new(Type::Any))),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::JsonWriteLines,
                ApiArgCheck::JsonCompatible,
            ),
        ),
    ])
}

fn linux_module() -> ModuleSig {
    let list_str = || Type::List(Box::new(Type::Str));
    let list_path = || Type::List(Box::new(Type::Path));
    module_sig(vec![
        (
            "write_device",
            sig(
                vec![param("device", Type::Path), param("source", Type::Path)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxWriteDevice,
            ),
        ),
        (
            "read_device",
            sig(
                vec![
                    param("device", Type::Path),
                    param("dest", Type::Path),
                    param("bytes", Type::Int),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxReadDevice,
            ),
        ),
        (
            "uevent_stream",
            sig(
                Vec::new(),
                result(Type::Stream(Box::new(linux_uevent_type()))),
                false,
                RuntimeOp::LinuxUeventStream,
            ),
        ),
        (
            "mount",
            sig(
                vec![
                    param("source", Type::Str),
                    param("target", Type::Path),
                    default_param("fstype", Type::Str),
                    default_param("options", list_str()),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxMount,
            ),
        ),
        (
            "mount_all",
            sig(
                Vec::new(),
                result(Type::Unit),
                false,
                RuntimeOp::LinuxMountAll,
            ),
        ),
        (
            "umount_all",
            sig(
                vec![default_param("types", list_str())],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxUmountAll,
            ),
        ),
        (
            "swapon_all",
            sig(
                Vec::new(),
                result(Type::Unit),
                false,
                RuntimeOp::LinuxSwaponAll,
            ),
        ),
        (
            "swapoff_all",
            sig(
                Vec::new(),
                result(Type::Unit),
                false,
                RuntimeOp::LinuxSwapoffAll,
            ),
        ),
        (
            "root_device",
            sig(
                Vec::new(),
                result(Type::Str),
                false,
                RuntimeOp::LinuxRootDevice,
            ),
        ),
        (
            "link_up",
            sig(
                vec![param("interface", Type::Str)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxLinkUp,
            ),
        ),
        (
            "link_down",
            sig(
                vec![param("interface", Type::Str)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxLinkDown,
            ),
        ),
        (
            "set_ipv4_address",
            sig(
                vec![
                    param("interface", Type::Str),
                    param("address", Type::Str),
                    param("netmask", Type::Str),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxSetIpv4Address,
            ),
        ),
        (
            "flush_ipv4_addresses",
            sig(
                vec![param("interface", Type::Str)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxFlushIpv4Addresses,
            ),
        ),
        (
            "add_default_ipv4_route",
            sig(
                vec![
                    param("gateway", Type::Str),
                    default_param("interface", Type::Str),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxAddDefaultIpv4Route,
            ),
        ),
        (
            "del_default_ipv4_route",
            sig(
                vec![param("gateway", Type::Str), param("interface", Type::Str)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxDelDefaultIpv4Route,
            ),
        ),
        (
            "dhcp_socket",
            sig(
                vec![param("interface", Type::Str)],
                result(Type::Int),
                false,
                RuntimeOp::LinuxDhcpSocket,
            ),
        ),
        (
            "dhcp_send",
            sig(
                vec![param("fd", Type::Int), param("payload", Type::Bytes)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxDhcpSend,
            ),
        ),
        (
            "dhcp_recv",
            sig(
                vec![param("fd", Type::Int), param("timeout_ms", Type::Int)],
                result(Type::Bytes),
                false,
                RuntimeOp::LinuxDhcpRecv,
            ),
        ),
        (
            "dhcp_close",
            sig(
                vec![param("fd", Type::Int)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxDhcpClose,
            ),
        ),
        (
            "dhcp_send_release",
            sig(
                vec![
                    param("interface", Type::Str),
                    param("address", Type::Str),
                    param("server_id", Type::Str),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxDhcpSendRelease,
            ),
        ),
        (
            "interfaces",
            sig(
                Vec::new(),
                result(Type::List(Box::new(linux_interface_type()))),
                false,
                RuntimeOp::LinuxInterfaces,
            ),
        ),
        (
            "routes",
            sig(
                Vec::new(),
                result(Type::List(Box::new(linux_route_type()))),
                false,
                RuntimeOp::LinuxRoutes,
            ),
        ),
        (
            "meminfo",
            sig(
                Vec::new(),
                result(linux_meminfo_type()),
                false,
                RuntimeOp::LinuxMemInfo,
            ),
        ),
        (
            "modules",
            sig(
                Vec::new(),
                result(Type::List(Box::new(linux_module_type()))),
                false,
                RuntimeOp::LinuxModules,
            ),
        ),
        (
            "dmesg",
            sig(Vec::new(), result(list_str()), false, RuntimeOp::LinuxDmesg),
        ),
        (
            "is_mountpoint",
            sig(
                vec![param("path", Type::Path)],
                result(Type::Bool),
                false,
                RuntimeOp::LinuxIsMountpoint,
            ),
        ),
        (
            "disk_usage",
            sig(
                vec![default_param("path", Type::Path)],
                result(Type::List(Box::new(linux_disk_usage_type()))),
                false,
                RuntimeOp::LinuxDiskUsage,
            ),
        ),
        (
            "block_devices",
            sig(
                Vec::new(),
                result(Type::List(Box::new(linux_block_device_type()))),
                false,
                RuntimeOp::LinuxBlockDevices,
            ),
        ),
        (
            "sysctl_get",
            sig(
                vec![param("key", Type::Str)],
                result(Type::Str),
                false,
                RuntimeOp::LinuxSysctlGet,
            ),
        ),
        (
            "sysctl_set",
            sig(
                vec![param("key", Type::Str), param("value", Type::Str)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxSysctlSet,
            ),
        ),
        (
            "file_attrs",
            sig(
                vec![param("path", Type::Path)],
                result(linux_file_attrs_type()),
                false,
                RuntimeOp::LinuxFileAttrs,
            ),
        ),
        (
            "set_file_attrs",
            sig(
                vec![param("path", Type::Path), param("flags", Type::Int)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxSetFileAttrs,
            ),
        ),
        (
            "file_version",
            sig(
                vec![param("path", Type::Path)],
                result(Type::Int),
                false,
                RuntimeOp::LinuxFileVersion,
            ),
        ),
        (
            "set_file_version",
            sig(
                vec![param("path", Type::Path), param("version", Type::Int)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxSetFileVersion,
            ),
        ),
        (
            "sysctl_load_dirs",
            sig(
                vec![
                    param("dirs", list_path()),
                    default_param("fallback", Type::Path),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxSysctlLoadDirs,
            ),
        ),
        (
            "kill_all",
            sig(
                vec![
                    default_param("signal", Type::Str),
                    default_param("except_pid1", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxKillAll,
            ),
        ),
        (
            "chroot",
            sig(
                vec![param("path", Type::Path)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxChroot,
            ),
        ),
        (
            "mknod",
            sig(
                vec![
                    param("path", Type::Path),
                    param("kind", Type::Str),
                    param("major", Type::Int),
                    param("minor", Type::Int),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxMknod,
            ),
        ),
        (
            "insmod",
            sig(
                vec![
                    param("path", Type::Path),
                    default_param("params", Type::Str),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxInsmod,
            ),
        ),
        (
            "rmmod",
            sig(
                vec![param("name", Type::Str), default_param("force", Type::Bool)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxRmmod,
            ),
        ),
        (
            "pivot_root",
            sig(
                vec![param("new_root", Type::Path), param("put_old", Type::Path)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxPivotRoot,
            ),
        ),
        (
            "switch_root",
            sig(
                vec![param("new_root", Type::Path), param("init", Type::Path)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxSwitchRoot,
            ),
        ),
        (
            "hwclock",
            sig(
                Vec::new(),
                result(Type::Int),
                false,
                RuntimeOp::LinuxHwclock,
            ),
        ),
        (
            "set_hwclock",
            sig(
                vec![param("epoch_ms", Type::Int)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxSetHwclock,
            ),
        ),
        (
            "set_system_clock",
            sig(
                vec![param("epoch_ms", Type::Int)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxSetSystemClock,
            ),
        ),
        (
            "rfkill_list",
            sig(
                Vec::new(),
                result(Type::List(Box::new(linux_rfkill_type()))),
                false,
                RuntimeOp::LinuxRfkillList,
            ),
        ),
        (
            "rfkill_block",
            sig(
                vec![param("id", Type::Int)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxRfkillBlock,
            ),
        ),
        (
            "rfkill_unblock",
            sig(
                vec![param("id", Type::Int)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxRfkillUnblock,
            ),
        ),
        (
            "loop_attach",
            sig(
                vec![
                    param("file", Type::Path),
                    default_param("device", Type::Path),
                ],
                result(Type::Path),
                false,
                RuntimeOp::LinuxLoopAttach,
            ),
        ),
        (
            "loop_detach",
            sig(
                vec![param("device", Type::Path)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxLoopDetach,
            ),
        ),
        (
            "loop_list",
            sig(
                Vec::new(),
                result(Type::List(Box::new(linux_loop_device_type()))),
                false,
                RuntimeOp::LinuxLoopList,
            ),
        ),
        (
            "mkswap",
            sig(
                vec![param("device", Type::Path)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxMkswap,
            ),
        ),
        (
            "swapon",
            sig(
                vec![
                    param("device", Type::Path),
                    default_param("priority", Type::Int),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxSwapon,
            ),
        ),
        (
            "swapoff",
            sig(
                vec![param("device", Type::Path)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxSwapoff,
            ),
        ),
        (
            "blkid",
            sig(
                vec![param("device", Type::Path)],
                result(linux_blkid_type()),
                false,
                RuntimeOp::LinuxBlkid,
            ),
        ),
        (
            "modinfo",
            sig(
                vec![param("name", Type::Str)],
                result(linux_modinfo_type()),
                false,
                RuntimeOp::LinuxModinfo,
            ),
        ),
        (
            "modprobe",
            sig(
                vec![param("name", Type::Str), default_param("params", Type::Str)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxModprobe,
            ),
        ),
        (
            "depmod",
            sig(
                vec![default_param("version", Type::Str)],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxDepmod,
            ),
        ),
        (
            "open_files",
            sig(
                vec![default_param("pid", Type::Int)],
                result(Type::List(Box::new(linux_open_file_type()))),
                false,
                RuntimeOp::LinuxOpenFiles,
            ),
        ),
        (
            "partition_table",
            sig(
                vec![param("device", Type::Path)],
                result(linux_partition_table_type()),
                false,
                RuntimeOp::LinuxPartitionTable,
            ),
        ),
        (
            "write_partition_table",
            sig(
                vec![
                    param("device", Type::Path),
                    param("table", Type::Record(BTreeMap::new())),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::LinuxWritePartitionTable,
            ),
        ),
        (
            "fsck",
            sig(
                vec![
                    param("device", Type::Path),
                    default_param("fstype", Type::Str),
                    default_param("repair", Type::Bool),
                ],
                result(linux_fsck_type()),
                false,
                RuntimeOp::LinuxFsck,
            ),
        ),
        (
            "halt",
            sig(Vec::new(), result(Type::Unit), false, RuntimeOp::LinuxHalt),
        ),
        (
            "poweroff",
            sig(
                Vec::new(),
                result(Type::Unit),
                false,
                RuntimeOp::LinuxPoweroff,
            ),
        ),
        (
            "reboot",
            sig(
                Vec::new(),
                result(Type::Unit),
                false,
                RuntimeOp::LinuxReboot,
            ),
        ),
    ])
}

fn path_module() -> ModuleSig {
    module_sig(vec![(
        "absolute",
        sig(
            vec![param("path", Type::Path)],
            result(Type::Path),
            false,
            RuntimeOp::PathAbsolute,
        ),
    )])
}

fn unix_module() -> ModuleSig {
    module_sig(vec![
        (
            "reap_child_events",
            sig(
                Vec::new(),
                result(Type::List(Box::new(unix_child_event_type()))),
                false,
                RuntimeOp::UnixReapChildEvents,
            ),
        ),
        (
            "pid1_setup",
            sig(
                vec![
                    param("signals", Type::List(Box::new(Type::Str))),
                    default_param("subreaper", Type::Bool),
                    default_param("allow_non_pid1", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::UnixPid1Setup,
            ),
        ),
        (
            "wait_pid1_event",
            sig(
                vec![default_param("timeout", Type::Duration)],
                result(unix_pid1_event_type()),
                false,
                RuntimeOp::UnixWaitPid1Event,
            ),
        ),
        (
            "shutdown_process_groups",
            sig(
                vec![
                    param("groups", Type::List(Box::new(Type::Int))),
                    param("term_timeout", Type::Duration),
                    default_param("kill_timeout", Type::Duration),
                ],
                result(unix_pid1_shutdown_type()),
                false,
                RuntimeOp::UnixShutdownProcessGroups,
            ),
        ),
        (
            "spawn_process_group",
            sig(
                vec![
                    param("command", Type::Command),
                    default_param("notify", Type::Bool),
                ],
                result(unix_spawned_child_type()),
                false,
                RuntimeOp::UnixSpawnProcessGroup,
            ),
        ),
        (
            "spawn_process_group_log",
            sig(
                vec![
                    param("command", Type::Command),
                    param("log", Type::Path),
                    default_param("notify", Type::Bool),
                ],
                result(unix_spawned_child_type()),
                false,
                RuntimeOp::UnixSpawnProcessGroupLog,
            ),
        ),
        (
            "spawn_logged_process_group",
            sig(
                vec![
                    param("command", Type::Command),
                    param("logger", Type::Command),
                ],
                result(unix_logged_process_group_type()),
                false,
                RuntimeOp::UnixSpawnLoggedProcessGroup,
            ),
        ),
        (
            "spawn_with_tty",
            sig(
                vec![param("command", Type::Command), param("tty", Type::Str)],
                result(unix_spawned_child_type()),
                false,
                RuntimeOp::UnixSpawnWithTty,
            ),
        ),
        (
            "notify_ready",
            sig(
                vec![param("fd", Type::Int)],
                result(Type::Bool),
                false,
                RuntimeOp::UnixNotifyReady,
            ),
        ),
        (
            "notify_close",
            sig(
                vec![param("fd", Type::Int)],
                result(Type::Unit),
                false,
                RuntimeOp::UnixNotifyClose,
            ),
        ),
        (
            "kill_process_group",
            sig(
                vec![param("pid", Type::Int), param("signal", Type::Str)],
                result(Type::Unit),
                false,
                RuntimeOp::UnixKillProcessGroup,
            ),
        ),
        (
            "exec",
            sig(
                vec![param("command", Type::Command)],
                result(Type::Unit),
                false,
                RuntimeOp::UnixExec,
            ),
        ),
        (
            "set_hostname",
            sig(
                vec![param("hostname", Type::Str)],
                result(Type::Unit),
                false,
                RuntimeOp::UnixSetHostname,
            ),
        ),
        (
            "uptime_seconds",
            sig(
                Vec::new(),
                result(Type::Int),
                false,
                RuntimeOp::UnixUptimeSeconds,
            ),
        ),
        (
            "tty",
            sig(Vec::new(), result(Type::Str), false, RuntimeOp::UnixTty),
        ),
        (
            "id",
            sig(Vec::new(), result(unix_id_type()), false, RuntimeOp::UnixId),
        ),
        (
            "tty_attrs",
            sig(
                vec![default_param("fd", Type::Int)],
                result(unix_tty_attrs_type()),
                false,
                RuntimeOp::UnixTtyAttrs,
            ),
        ),
        (
            "set_tty_attrs",
            sig(
                vec![
                    param("attrs", Type::Record(BTreeMap::new())),
                    default_param("fd", Type::Int),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::UnixSetTtyAttrs,
            ),
        ),
        (
            "kill_all",
            sig(
                vec![param("name", Type::Str), default_param("signal", Type::Str)],
                result(unix_kill_all_result_type()),
                false,
                RuntimeOp::UnixKillAll,
            ),
        ),
    ])
}

fn process_module() -> ModuleSig {
    module_sig(vec![
        (
            "list",
            sig(
                Vec::new(),
                result(Type::Stream(Box::new(process_entry_type()))),
                false,
                RuntimeOp::ProcessList,
            ),
        ),
        (
            "threads",
            sig(
                Vec::new(),
                result(Type::Stream(Box::new(process_thread_type()))),
                false,
                RuntimeOp::ProcessThreads,
            ),
        ),
        (
            "threads",
            sig(
                vec![param("pid", Type::Int)],
                result(Type::Stream(Box::new(process_thread_type()))),
                false,
                RuntimeOp::ProcessThreads,
            ),
        ),
        (
            "current_pid",
            sig(
                Vec::new(),
                result(Type::Int),
                false,
                RuntimeOp::ProcessCurrentPid,
            ),
        ),
        (
            "stats",
            sig(
                vec![param("pid", Type::Int)],
                result(process_stats_type()),
                false,
                RuntimeOp::ProcessStats,
            ),
        ),
        (
            "which",
            sig(
                vec![param("name", Type::Str)],
                result(Type::Path),
                false,
                RuntimeOp::ProcessWhich,
            ),
        ),
        (
            "port",
            sig(
                vec![param("port", Type::Int)],
                result(Type::Stream(Box::new(process_port_type()))),
                false,
                RuntimeOp::ProcessPort,
            ),
        ),
        (
            "ports",
            sig(
                Vec::new(),
                result(Type::Stream(Box::new(process_port_type()))),
                false,
                RuntimeOp::ProcessPorts,
            ),
        ),
        (
            "ports",
            sig(
                vec![param("pid", Type::Int)],
                result(Type::Stream(Box::new(process_port_type()))),
                false,
                RuntimeOp::ProcessPortsForPid,
            ),
        ),
        (
            "signal",
            sig(
                vec![param("signal", Type::Str)],
                result(signal_record_type()),
                false,
                RuntimeOp::ProcessSignal,
            ),
        ),
        (
            "kill",
            sig(
                vec![param("pid", Type::Int), default_param("signal", Type::Str)],
                result(Type::Unit),
                false,
                RuntimeOp::ProcessKill,
            ),
        ),
        (
            "argv_words",
            sig(
                vec![param("text", Type::Str)],
                result(Type::List(Box::new(Type::Str))),
                true,
                RuntimeOp::ProcessArgvWords,
            ),
        ),
        (
            "command_argv",
            sig(
                vec![
                    param("target", Type::Str),
                    param("argv", Type::List(Box::new(Type::Str))),
                    default_param("cwd", Type::Path),
                    default_param("env", Type::Record(BTreeMap::new())),
                    default_param("stdin", Type::Path),
                    default_param("stdout", Type::Path),
                    default_param("stderr", Type::Path),
                    default_param("stdout_append", Type::Bool),
                    default_param("stderr_append", Type::Bool),
                    default_param("timeout", Type::Duration),
                    default_param("detach", Type::Bool),
                    default_param("new_session", Type::Bool),
                    default_param("ignore_hup", Type::Bool),
                    default_param("cpu_max", Type::Int),
                ],
                Type::Command,
                true,
                RuntimeOp::ProcessCommandArgv,
            ),
        ),
        (
            "command_argv",
            sig(
                vec![
                    param("target", Type::Str),
                    param("argv", Type::List(Box::new(Type::Path))),
                    default_param("cwd", Type::Path),
                    default_param("env", Type::Record(BTreeMap::new())),
                    default_param("stdin", Type::Path),
                    default_param("stdout", Type::Path),
                    default_param("stderr", Type::Path),
                    default_param("stdout_append", Type::Bool),
                    default_param("stderr_append", Type::Bool),
                    default_param("timeout", Type::Duration),
                    default_param("detach", Type::Bool),
                    default_param("new_session", Type::Bool),
                    default_param("ignore_hup", Type::Bool),
                    default_param("cpu_max", Type::Int),
                ],
                Type::Command,
                true,
                RuntimeOp::ProcessCommandArgv,
            ),
        ),
        (
            "command_argv",
            sig(
                vec![
                    param("target", Type::Path),
                    param("argv", Type::List(Box::new(Type::Str))),
                    default_param("cwd", Type::Path),
                    default_param("env", Type::Record(BTreeMap::new())),
                    default_param("stdin", Type::Path),
                    default_param("stdout", Type::Path),
                    default_param("stderr", Type::Path),
                    default_param("stdout_append", Type::Bool),
                    default_param("stderr_append", Type::Bool),
                    default_param("timeout", Type::Duration),
                    default_param("detach", Type::Bool),
                    default_param("new_session", Type::Bool),
                    default_param("ignore_hup", Type::Bool),
                    default_param("cpu_max", Type::Int),
                ],
                Type::Command,
                true,
                RuntimeOp::ProcessCommandArgv,
            ),
        ),
        (
            "command_argv",
            sig(
                vec![
                    param("target", Type::Path),
                    param("argv", Type::List(Box::new(Type::Path))),
                    default_param("cwd", Type::Path),
                    default_param("env", Type::Record(BTreeMap::new())),
                    default_param("stdin", Type::Path),
                    default_param("stdout", Type::Path),
                    default_param("stderr", Type::Path),
                    default_param("stdout_append", Type::Bool),
                    default_param("stderr_append", Type::Bool),
                    default_param("timeout", Type::Duration),
                    default_param("detach", Type::Bool),
                    default_param("new_session", Type::Bool),
                    default_param("ignore_hup", Type::Bool),
                    default_param("cpu_max", Type::Int),
                ],
                Type::Command,
                true,
                RuntimeOp::ProcessCommandArgv,
            ),
        ),
        (
            "run",
            sig(
                vec![param("command", Type::Command)],
                Type::Result(Box::new(Type::Status), Box::new(Type::ProcessError)),
                false,
                RuntimeOp::ProcessRun,
            ),
        ),
        (
            "spawn",
            sig(
                vec![param("command", Type::Command)],
                result(spawn_record_type()),
                false,
                RuntimeOp::ProcessSpawn,
            ),
        ),
        (
            "wait_any",
            sig(
                vec![param("handles", Type::List(Box::new(Type::ProcessHandle)))],
                Type::Result(
                    Box::new(process_wait_any_type()),
                    Box::new(Type::ProcessError),
                ),
                false,
                RuntimeOp::ProcessWaitAny,
            ),
        ),
        (
            "wait_ready",
            sig(
                vec![param("handles", Type::List(Box::new(Type::ProcessHandle)))],
                Type::Result(
                    Box::new(Type::List(Box::new(process_wait_any_type()))),
                    Box::new(Type::ProcessError),
                ),
                false,
                RuntimeOp::ProcessWaitReady,
            ),
        ),
        (
            "command",
            sig(Vec::new(), Type::Command, false, RuntimeOp::ProcessCommand),
        ),
    ])
}

fn system_module() -> ModuleSig {
    module_sig(vec![
        (
            "hostname",
            sig(
                Vec::new(),
                result(Type::Str),
                false,
                RuntimeOp::SystemHostname,
            ),
        ),
        (
            "uname",
            sig(
                Vec::new(),
                result(uname_record_type()),
                false,
                RuntimeOp::SystemUname,
            ),
        ),
        (
            "memory",
            sig(
                Vec::new(),
                result(system_memory_type()),
                false,
                RuntimeOp::SystemMemory,
            ),
        ),
        (
            "os_release",
            sig(
                Vec::new(),
                result(system_os_release_type()),
                false,
                RuntimeOp::SystemOsRelease,
            ),
        ),
    ])
}

fn test_module() -> ModuleSig {
    let unknown = || Type::Any;
    let record = || Type::Record(BTreeMap::new());
    module_sig(vec![
        (
            "ok",
            sig(
                vec![
                    param("condition", Type::Bool),
                    default_param("message", Type::Str),
                ],
                result(Type::Unit),
                true,
                RuntimeOp::TestOk,
            ),
        ),
        (
            "eq",
            sig(
                vec![
                    param("left", unknown()),
                    param("right", unknown()),
                    default_param("message", Type::Str),
                ],
                result(Type::Unit),
                true,
                RuntimeOp::TestEq,
            ),
        ),
        (
            "ne",
            sig(
                vec![
                    param("left", unknown()),
                    param("right", unknown()),
                    default_param("message", Type::Str),
                ],
                result(Type::Unit),
                true,
                RuntimeOp::TestNe,
            ),
        ),
        (
            "contains",
            sig(
                vec![
                    param("haystack", unknown()),
                    param("needle", unknown()),
                    default_param("message", Type::Str),
                ],
                result(Type::Unit),
                true,
                RuntimeOp::TestContains,
            ),
        ),
        (
            "not_contains",
            sig(
                vec![
                    param("haystack", unknown()),
                    param("needle", unknown()),
                    default_param("message", Type::Str),
                ],
                result(Type::Unit),
                true,
                RuntimeOp::TestNotContains,
            ),
        ),
        (
            "error_kind",
            sig(
                vec![
                    param("value", unknown()),
                    param("kind", Type::Str),
                    default_param("message", Type::Str),
                ],
                result(Type::Unit),
                true,
                RuntimeOp::TestErrorKind,
            ),
        ),
        (
            "fail",
            sig(
                vec![default_param("message", Type::Str)],
                result(Type::Unit),
                true,
                RuntimeOp::TestFail,
            ),
        ),
        (
            "skip",
            sig(
                vec![default_param("message", Type::Str)],
                result(Type::Unit),
                true,
                RuntimeOp::TestSkip,
            ),
        ),
        (
            "temp_path",
            sig(
                vec![
                    param("ctx", test_context_type()),
                    default_param("name", Type::Str),
                ],
                Type::Path,
                false,
                RuntimeOp::TestTempPath,
            ),
        ),
        (
            "temp_dir",
            sig(
                vec![
                    param("ctx", test_context_type()),
                    default_param("name", Type::Str),
                ],
                result(Type::Path),
                false,
                RuntimeOp::TestTempDir,
            ),
        ),
        (
            "temp_file",
            sig(
                vec![
                    param("ctx", test_context_type()),
                    default_param("name", Type::Str),
                    default_param("contents", Type::Bytes),
                ],
                result(Type::Path),
                false,
                RuntimeOp::TestTempFile,
            ),
        ),
        (
            "mock",
            sig(
                vec![
                    param("ctx", test_context_type()),
                    param("op", Type::Str),
                    param("matcher", record()),
                    param("result", unknown()),
                    default_param("times", Type::Int),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::TestMock,
            ),
        ),
        (
            "calls",
            sig(
                vec![
                    param("ctx", test_context_type()),
                    default_param("op", Type::Str),
                ],
                Type::List(Box::new(test_call_type())),
                false,
                RuntimeOp::TestCalls,
            ),
        ),
        (
            "run_script",
            sig(
                vec![
                    param("ctx", test_context_type()),
                    param("source", Type::Str),
                    default_param("args", Type::List(Box::new(Type::Str))),
                    default_param("env", Type::Record(BTreeMap::new())),
                    default_param("stdin", Type::Bytes),
                    default_param("name", Type::Str),
                ],
                result(test_script_output_type()),
                false,
                RuntimeOp::TestRunScript,
            ),
        ),
        (
            "run_xsh",
            sig(
                vec![
                    param("ctx", test_context_type()),
                    param("source", Type::Str),
                    default_param("xsh_args", Type::List(Box::new(Type::Str))),
                    default_param("script_args", Type::List(Box::new(Type::Str))),
                    default_param("env", Type::Record(BTreeMap::new())),
                    default_param("stdin", Type::Bytes),
                    default_param("name", Type::Str),
                ],
                result(test_script_output_type()),
                false,
                RuntimeOp::TestRunXsh,
            ),
        ),
        (
            "run_xsht_trace",
            sig(
                vec![
                    param("ctx", test_context_type()),
                    param("source", Type::Str),
                    default_param("trace_args", Type::List(Box::new(Type::Str))),
                    default_param("script_args", Type::List(Box::new(Type::Str))),
                    default_param("env", Type::Record(BTreeMap::new())),
                    default_param("stdin", Type::Bytes),
                    default_param("name", Type::Str),
                ],
                result(test_script_output_type()),
                false,
                RuntimeOp::TestRunXshtTrace,
            ),
        ),
    ])
}

fn time_module() -> ModuleSig {
    module_sig(vec![
        ("now", sig(Vec::new(), Type::Int, false, RuntimeOp::TimeNow)),
        (
            "sleep",
            sig(
                vec![param("duration", Type::Duration)],
                result(Type::Unit),
                false,
                RuntimeOp::TimeSleep,
            ),
        ),
        (
            "millis",
            sig(
                vec![param("ms", Type::Int)],
                Type::Duration,
                true,
                RuntimeOp::TimeMillis,
            ),
        ),
        (
            "seconds",
            sig(
                vec![param("seconds", Type::Int)],
                Type::Duration,
                true,
                RuntimeOp::TimeSeconds,
            ),
        ),
        (
            "measure",
            sig(
                vec![
                    param("command", Type::Command),
                    default_param("quiet", Type::Bool),
                ],
                result(measured_command_type()),
                false,
                RuntimeOp::TimeMeasure,
            ),
        ),
        (
            "format",
            sig(
                vec![
                    param("epoch_ms", Type::Int),
                    param("format", Type::Str),
                    default_param("utc", Type::Bool),
                ],
                result(Type::Str),
                false,
                RuntimeOp::TimeFormat,
            ),
        ),
        (
            "duration_compact",
            sig(
                vec![param("seconds", Type::Int)],
                Type::Str,
                true,
                RuntimeOp::TimeDurationCompact,
            ),
        ),
    ])
}

fn tui_module() -> ModuleSig {
    module_sig(vec![
        (
            "reset",
            sig(Vec::new(), Type::Str, true, RuntimeOp::TuiReset),
        ),
        ("bold", sig(Vec::new(), Type::Str, true, RuntimeOp::TuiBold)),
        ("dim", sig(Vec::new(), Type::Str, true, RuntimeOp::TuiDim)),
        ("red", sig(Vec::new(), Type::Str, true, RuntimeOp::TuiRed)),
        (
            "green",
            sig(Vec::new(), Type::Str, true, RuntimeOp::TuiGreen),
        ),
        (
            "yellow",
            sig(Vec::new(), Type::Str, true, RuntimeOp::TuiYellow),
        ),
        ("blue", sig(Vec::new(), Type::Str, true, RuntimeOp::TuiBlue)),
        (
            "magenta",
            sig(Vec::new(), Type::Str, true, RuntimeOp::TuiMagenta),
        ),
        ("cyan", sig(Vec::new(), Type::Str, true, RuntimeOp::TuiCyan)),
        (
            "white",
            sig(Vec::new(), Type::Str, true, RuntimeOp::TuiWhite),
        ),
        ("gray", sig(Vec::new(), Type::Str, true, RuntimeOp::TuiGray)),
        (
            "clear",
            sig(Vec::new(), Type::Str, true, RuntimeOp::TuiClear),
        ),
        ("home", sig(Vec::new(), Type::Str, true, RuntimeOp::TuiHome)),
        (
            "erase_line",
            sig(Vec::new(), Type::Str, true, RuntimeOp::TuiEraseLine),
        ),
        (
            "hide_cursor",
            sig(Vec::new(), Type::Str, true, RuntimeOp::TuiHideCursor),
        ),
        (
            "show_cursor",
            sig(Vec::new(), Type::Str, true, RuntimeOp::TuiShowCursor),
        ),
        (
            "left_pad",
            sig(
                vec![param("text", Type::Str), param("width", Type::Int)],
                Type::Str,
                true,
                RuntimeOp::TuiLeftPad,
            ),
        ),
        (
            "right_pad",
            sig(
                vec![param("text", Type::Str), param("width", Type::Int)],
                Type::Str,
                true,
                RuntimeOp::TuiRightPad,
            ),
        ),
        (
            "read_secret",
            sig(
                vec![param("prompt", Type::Str)],
                result(Type::Str),
                false,
                RuntimeOp::TuiReadSecret,
            ),
        ),
    ])
}

fn user_module() -> ModuleSig {
    module_sig(vec![
        (
            "current",
            sig(
                Vec::new(),
                result(user_record_type()),
                false,
                RuntimeOp::UserCurrent,
            ),
        ),
        (
            "lookup",
            sig(
                vec![param("name", Type::Str)],
                result(user_record_type()),
                false,
                RuntimeOp::UserLookup,
            ),
        ),
        (
            "by_uid",
            sig(
                vec![param("uid", Type::Int)],
                result(user_record_type()),
                false,
                RuntimeOp::UserByUid,
            ),
        ),
        (
            "add",
            sig(
                vec![
                    param("name", Type::Str),
                    default_param("uid", Type::Int),
                    default_param("gid", Type::Int),
                    default_param("home", Type::Path),
                    default_param("shell", Type::Path),
                    default_param("gecos", Type::Str),
                ],
                result(user_record_type()),
                false,
                RuntimeOp::UserAdd,
            ),
        ),
        (
            "remove",
            sig(
                vec![
                    param("name", Type::Str),
                    default_param("remove_home", Type::Bool),
                ],
                result(Type::Unit),
                false,
                RuntimeOp::UserRemove,
            ),
        ),
    ])
}

fn utils_module() -> ModuleSig {
    module_sig(vec![(
        "cache",
        sig(
            vec![
                param("fn", Type::Any),
                default_param("args", Type::List(Box::new(Type::Any))),
            ],
            Type::Any,
            false,
            RuntimeOp::UtilsCache,
        ),
    )])
}
