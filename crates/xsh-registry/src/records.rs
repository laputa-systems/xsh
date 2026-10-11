use crate::types::Type;
mod linux_storage;
mod net_sockets;
mod sampling;
pub use linux_storage::{
    linux_ata_result_type, linux_ata_smart_status_type, linux_nvme_command_type, linux_sg_io_type,
    linux_storage_candidate_type,
};
pub use net_sockets::{
    NET_CONSTANTS, linux_accepted_socket_type, linux_control_message_type,
    linux_net_constants_type, linux_netlink_message_type, linux_received_message_type,
    linux_socket_address_argument_type, linux_socket_address_type,
};
pub use sampling::{linux_sample_type, linux_cpu_sample_type, linux_disk_sample_type, linux_process_sample_type};
use std::collections::BTreeMap;
use std::sync::LazyLock;

fn btree_map<K: Ord, V>(entries: Vec<(K, V)>) -> BTreeMap<K, V> {
    let mut map = BTreeMap::new();
    map.extend(entries);
    map
}

fn name_type_map<K: Into<String>>(entries: Vec<(K, Type)>) -> BTreeMap<String, Type> {
    entries
        .into_iter()
        .map(|(name, ty)| (name.into(), ty))
        .collect()
}

pub fn record_schemas() -> BTreeMap<&'static str, Type> {
    btree_map(vec![
        ("ArchiveEntry", archive_entry_type()),
        ("DiffResult", diff_result_type()),
        ("DnsHost", dns_host_type()),
        ("DnsLookup", dns_lookup_type()),
        ("ElfDynamicTag", elf_dynamic_tag_type()),
        ("ElfInfo", elf_info_type()),
        ("EnvEntry", env_entry_type()),
        ("EnvRawEntry", env_raw_entry_type()),
        ("EnvPathEntry", env_path_entry_type()),
        ("FsCopyFileResult", fs_copy_file_result_type()),
        ("FsCopyTreeResult", fs_copy_tree_result_type()),
        ("FsDataRange", fs_data_range_type()),
        ("FsEntry", fs_entry_type()),
        ("FsFilesystemStats", fs_filesystem_stats_type()),
        ("FsRootFilesystemStats", fs_root_filesystem_stats_type()),
        ("FsStat", fs_stat_type()),
        ("FsStatvfs", fs_statvfs_type()),
        ("FsMount", fs_mount_type()),
        ("FsRemoveManifestResult", fs_remove_manifest_result_type()),
        ("FsRootChildrenResult", fs_root_children_result_type()),
        ("FsRootReadResult", fs_root_read_result_type()),
        ("FsRootReadlinkResult", fs_root_readlink_result_type()),
        ("Group", group_record_type()),
        ("LinuxBlockDevice", linux_block_device_type()),
        ("LinuxDiskUsage", linux_disk_usage_type()),
        ("LinuxFileAttrs", linux_file_attrs_type()),
        ("LinuxInterface", linux_interface_type()),
        ("LinuxInterfaceAddress", linux_interface_address_type()),
        ("LinuxRoute", linux_route_type()),
        ("LinuxAcceptedSocket", linux_accepted_socket_type()),
        ("LinuxControlMessage", linux_control_message_type()),
        ("LinuxNetConstants", linux_net_constants_type()),
        ("LinuxNetlinkAttribute", linux_netlink_attribute_type()),
        ("LinuxNetlinkMessage", linux_netlink_message_type()),
        ("LinuxReceivedMessage", linux_received_message_type()),
        ("LinuxSocketAddress", linux_socket_address_type()),
        ("LinuxNetworkAddress", linux_network_address_type()),
        ("LinuxNetworkIssue", linux_network_issue_type()),
        ("LinuxNetworkLink", linux_network_link_type()),
        ("LinuxNetworkNexthop", linux_network_nexthop_type()),
        ("LinuxNetworkRoute", linux_network_route_type()),
        ("LinuxNetworkRule", linux_network_rule_type()),
        ("LinuxNetworkDump", linux_network_dump_type()),
        ("LinuxLoopDevice", linux_loop_device_type()),
        ("LinuxNamespace", linux_namespace_type()),
        ("LinuxMemInfo", linux_meminfo_type()),
        ("LinuxSample", linux_sample_type()),
        ("LinuxCpuSample", linux_cpu_sample_type()),
        ("LinuxDiskSample", linux_disk_sample_type()),
        ("LinuxProcessSample", linux_process_sample_type()),
        ("LinuxModulePlan", linux_module_plan_type()),
        ("LinuxBlockdevInfo", linux_blockdev_info_type()),
        ("LinuxPrivileges", linux_privileges_type()),
        ("LinuxSgIo", linux_sg_io_type()),
        ("LinuxAtaResult", linux_ata_result_type()),
        ("LinuxAtaSmartStatus", linux_ata_smart_status_type()),
        ("LinuxNvmeCommand", linux_nvme_command_type()),
        ("LinuxStorageCandidate", linux_storage_candidate_type()),
        ("LinuxBlkid", linux_blkid_type()),
        ("LinuxFsck", linux_fsck_type()),
        ("LinuxModinfo", linux_modinfo_type()),
        ("LinuxModule", linux_module_type()),
        ("LinuxModuleParam", linux_module_param_type()),
        ("LinuxOpenFile", linux_open_file_type()),
        ("LinuxPartition", linux_partition_type()),
        ("LinuxPartitionTable", linux_partition_table_type()),
        ("LinuxRfkill", linux_rfkill_type()),
        ("LinuxUevent", linux_uevent_type()),
        ("MeasuredCommand", measured_command_type()),
        ("TimeCalendar", time_calendar_type()),
        ("MimeInfo", mime_info_type()),
        ("MimeParse", mime_parse_type()),
        ("NetHeader", net_header_type()),
        ("NetPool", net_pool_type()),
        ("NetResponse", net_response_type(true)),
        ("PatchResult", patch_result_type()),
        ("ProcessEntry", process_entry_type()),
        ("ProcessPort", process_port_type()),
        ("ProcessThread", process_thread_type()),
        ("ProcessIoPriority", process_io_priority_type()),
        ("ProcessScheduler", process_scheduler_type()),
        ("ProcessSchedulerRange", process_scheduler_range_type()),
        ("Rlimit", rlimit_type()),
        ("Signal", signal_record_type()),
        ("Spawn", spawn_record_type()),
        ("SystemMemory", system_memory_type()),
        ("SystemExecutionUnits", system_execution_units_type()),
        ("SystemOsRelease", system_os_release_type()),
        #[cfg(feature = "native-tests")]
        ("TestCall", test_call_type()),
        #[cfg(feature = "native-tests")]
        ("TestContext", test_context_type()),
        ("Uname", uname_record_type()),
        ("UnixChildEvent", unix_child_event_type()),
        ("UnixGroupId", unix_group_id_type()),
        ("UnixId", unix_id_type()),
        ("UnixKillAllResult", unix_kill_all_result_type()),
        ("UnixLoadAverage", unix_load_average_type()),
        ("UnixLoggedProcessGroup", unix_logged_process_group_type()),
        ("UnixPid1Event", unix_pid1_event_type()),
        ("UnixPid1Shutdown", unix_pid1_shutdown_type()),
        ("UnixPty", unix_pty_type()),
        ("UnixSpawnedChild", unix_spawned_child_type()),
        ("UnixTtyAttrs", unix_tty_attrs_type()),
        ("UnixTtyChar", unix_tty_char_type()),
        ("UnixTtyFlag", unix_tty_flag_type()),
        ("UnixTtyTable", unix_tty_table_type()),
        ("UnixUtmp", unix_utmp_type()),
        ("UnixWindowSize", unix_window_size_type()),
        ("User", user_record_type()),
    ])
}
static RECORD_SCHEMAS: LazyLock<BTreeMap<&'static str, Type>> = LazyLock::new(record_schemas);

pub fn standard_record_type(name: &str) -> Option<Type> {
    RECORD_SCHEMAS.get(name).cloned()
}

pub fn fs_entry_type() -> Type {
    Type::Record(name_type_map(vec![
        ("path".to_string(), Type::Path),
        ("blocks_512".to_string(), Type::Int),
        ("executable".to_string(), Type::Bool),
        ("name".to_string(), Type::Str),
        ("kind".to_string(), Type::Str),
        ("ext".to_string(), Type::Str),
        ("group_executable".to_string(), Type::Bool),
        ("size".to_string(), Type::Int),
        ("mode".to_string(), Type::Int),
        ("other_executable".to_string(), Type::Bool),
        ("owner_executable".to_string(), Type::Bool),
        ("uid".to_string(), Type::Int),
        ("gid".to_string(), Type::Int),
        ("modified".to_string(), Type::Int),
        ("accessed".to_string(), Type::Int),
        ("setgid".to_string(), Type::Bool),
        ("setuid".to_string(), Type::Bool),
        ("sticky".to_string(), Type::Bool),
        ("world_writable".to_string(), Type::Bool),
    ]))
}

pub fn env_path_entry_type() -> Type {
    Type::Record(name_type_map(vec![
        ("index".to_string(), Type::Int),
        ("raw".to_string(), Type::Str),
        ("path".to_string(), Type::Path),
        ("empty".to_string(), Type::Bool),
    ]))
}

pub fn fs_filesystem_stats_type() -> Type {
    Type::Record(name_type_map(vec![
        ("blocks_1k".to_string(), Type::Int),
        ("used_1k".to_string(), Type::Int),
        ("available_1k".to_string(), Type::Int),
        ("capacity_percent".to_string(), Type::Int),
    ]))
}

pub fn fs_root_filesystem_stats_type() -> Type {
    Type::Record(name_type_map(vec![
        ("state".to_string(), Type::Str),
        (
            "total_bytes".to_string(),
            Type::Optional(Box::new(Type::Int)),
        ),
        (
            "used_bytes".to_string(),
            Type::Optional(Box::new(Type::Int)),
        ),
        (
            "available_bytes".to_string(),
            Type::Optional(Box::new(Type::Int)),
        ),
        (
            "block_size_bytes".to_string(),
            Type::Optional(Box::new(Type::Int)),
        ),
        ("errno".to_string(), Type::Optional(Box::new(Type::Int))),
        (
            "error_kind".to_string(),
            Type::Optional(Box::new(Type::Str)),
        ),
    ]))
}

pub fn fs_mount_type() -> Type {
    Type::Record(name_type_map(vec![
        ("filesystem".to_string(), Type::Str),
        ("mounted_on".to_string(), Type::Path),
        ("fstype".to_string(), Type::Str),
        ("device".to_string(), Type::Int),
        ("blocks_1k".to_string(), Type::Int),
        ("used_1k".to_string(), Type::Int),
        ("available_1k".to_string(), Type::Int),
        ("capacity_percent".to_string(), Type::Int),
        ("files".to_string(), Type::Int),
        ("files_used".to_string(), Type::Int),
        ("files_free".to_string(), Type::Int),
        ("files_capacity_percent".to_string(), Type::Int),
        ("readonly".to_string(), Type::Bool),
    ]))
}

pub fn fs_stat_type() -> Type {
    Type::Record(name_type_map(vec![
        ("kind", Type::Str),
        ("mode", Type::Int),
        ("size", Type::Int),
        ("blocks_512", Type::Int),
        ("blksize", Type::Int),
        ("uid", Type::Int),
        ("gid", Type::Int),
        ("nlink", Type::Int),
        ("dev", Type::Int),
        ("ino", Type::Int),
        ("rdev", Type::Int),
        ("atime_ns", Type::Int),
        ("mtime_ns", Type::Int),
        ("ctime_ns", Type::Int),
        ("birth_ns", Type::Optional(Box::new(Type::Int))),
    ]))
}

pub fn fs_statvfs_type() -> Type {
    Type::Record(name_type_map(vec![
        ("block_size", Type::Int),
        ("fragment_size", Type::Int),
        ("blocks", Type::Int),
        ("blocks_free", Type::Int),
        ("blocks_available", Type::Int),
        ("files", Type::Int),
        ("files_free", Type::Int),
        ("files_available", Type::Int),
        ("fsid", Type::Int),
        ("name_max", Type::Int),
        ("type_magic", Type::Optional(Box::new(Type::Int))),
        ("flags", Type::Int),
        ("readonly", Type::Bool),
        ("nosuid", Type::Bool),
        ("nodev", Type::Bool),
        ("noexec", Type::Bool),
    ]))
}

pub fn fs_copy_file_result_type() -> Type {
    Type::Record(name_type_map(vec![
        ("bytes", Type::Int),
        ("hole_bytes", Type::Int),
        ("method", Type::Str),
        ("destination_replaced", Type::Bool),
    ]))
}

pub fn fs_data_range_type() -> Type {
    Type::Record(name_type_map(vec![
        ("offset", Type::Int),
        ("length", Type::Int),
    ]))
}

pub fn archive_entry_type() -> Type {
    Type::Record(name_type_map(vec![
        ("path".to_string(), Type::Path),
        ("kind".to_string(), Type::Str),
        ("size".to_string(), Type::Int),
        ("mode".to_string(), Type::Int),
        ("modified".to_string(), Type::Int),
        ("link_name".to_string(), Type::Str),
    ]))
}

#[allow(clippy::single_call_fn)]
pub fn regex_match_type() -> Type {
    Type::Record(name_type_map(vec![
        ("start".to_string(), Type::Int),
        ("end".to_string(), Type::Int),
        ("text".to_string(), Type::Str),
    ]))
}

pub fn diff_result_type() -> Type {
    Type::Record(name_type_map(vec![
        ("files".to_string(), Type::Int),
        ("hunks".to_string(), Type::Int),
        ("text".to_string(), Type::Str),
    ]))
}

pub fn dns_lookup_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("record".to_string(), Type::Str),
        ("value".to_string(), Type::Str),
        ("ttl".to_string(), Type::Int),
    ]))
}

pub fn dns_host_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("family".to_string(), Type::Str),
        ("addr".to_string(), Type::Str),
    ]))
}

pub fn elf_dynamic_tag_type() -> Type {
    Type::Record(name_type_map(vec![
        ("tag".to_string(), Type::Str),
        ("value".to_string(), Type::Int),
    ]))
}

pub fn elf_info_type() -> Type {
    Type::Record(name_type_map(vec![
        ("path".to_string(), Type::Path),
        ("class".to_string(), Type::Str),
        ("endian".to_string(), Type::Str),
        ("machine".to_string(), Type::Str),
        ("os_abi".to_string(), Type::Str),
        ("type".to_string(), Type::Str),
        ("interpreter".to_string(), Type::Str),
        ("soname".to_string(), Type::Str),
        ("needed".to_string(), Type::List(Box::new(Type::Str))),
        ("rpath".to_string(), Type::Str),
        ("runpath".to_string(), Type::Str),
        ("flags".to_string(), Type::List(Box::new(Type::Str))),
        (
            "dynamic_tags".to_string(),
            Type::List(Box::new(elf_dynamic_tag_type())),
        ),
    ]))
}

pub fn net_header_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("value".to_string(), Type::Str),
    ]))
}

/// One parsed checksum-file line from `hash.parse_check_line`.
pub fn hash_check_line_type() -> Type {
    Type::Record(name_type_map(vec![
        ("hex".to_string(), Type::Str),
        ("path".to_string(), Type::Str),
        ("binary".to_string(), Type::Bool),
    ]))
}

pub fn mime_info_type() -> Type {
    Type::Record(name_type_map(vec![
        ("mime".to_string(), Type::Str),
        ("exts".to_string(), Type::List(Box::new(Type::Str))),
    ]))
}

pub fn mime_parse_type() -> Type {
    Type::Record(name_type_map(vec![
        ("type".to_string(), Type::Str),
        (
            "params".to_string(),
            Type::Map(Box::new(Type::Str), Box::new(Type::Str)),
        ),
    ]))
}

pub fn net_response_type(include_body: bool) -> Type {
    let mut fields = name_type_map(vec![
        ("status".to_string(), Type::Int),
        ("reason".to_string(), Type::Str),
        ("bytes".to_string(), Type::Int),
        (
            "headers".to_string(),
            Type::List(Box::new(net_header_type())),
        ),
        ("url".to_string(), Type::Str),
        ("effective_url".to_string(), Type::Str),
        ("redirect_count".to_string(), Type::Int),
    ]);
    if include_body {
        fields.insert("body".to_string(), Type::Bytes);
    }
    Type::Record(fields)
}

pub fn net_pool_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("max_idle_per_host".to_string(), Type::Int),
        ("idle_timeout_ms".to_string(), Type::Int),
    ]))
}

pub fn patch_result_type() -> Type {
    Type::Record(name_type_map(vec![
        ("files".to_string(), Type::Int),
        ("hunks".to_string(), Type::Int),
    ]))
}

pub fn fs_copy_tree_result_type() -> Type {
    Type::Record(name_type_map(vec![
        ("files".to_string(), Type::Int),
        ("dirs".to_string(), Type::Int),
        ("symlinks".to_string(), Type::Int),
    ]))
}

pub fn fs_lock_type() -> Type {
    Type::FsLock
}

pub fn fs_lock_fields() -> BTreeMap<String, Type> {
    name_type_map(vec![
        ("path".to_string(), Type::Path),
        ("shared".to_string(), Type::Bool),
    ])
}

pub fn fs_root_type() -> Type {
    Type::FsRoot
}

pub fn fs_root_children_result_type() -> Type {
    Type::Record(name_type_map(vec![
        ("state", Type::Str),
        ("enumeration_succeeded", Type::Bool),
        ("children", Type::List(Box::new(Type::Path))),
        ("errno", Type::Optional(Box::new(Type::Int))),
        ("error_kind", Type::Optional(Box::new(Type::Str))),
    ]))
}

pub fn fs_root_read_result_type() -> Type {
    Type::Record(name_type_map(vec![
        ("state".to_string(), Type::Str),
        ("data".to_string(), Type::Optional(Box::new(Type::Bytes))),
        ("errno".to_string(), Type::Optional(Box::new(Type::Int))),
        (
            "error_kind".to_string(),
            Type::Optional(Box::new(Type::Str)),
        ),
        ("truncated".to_string(), Type::Bool),
    ]))
}

pub fn fs_root_readlink_result_type() -> Type {
    Type::Record(name_type_map(vec![
        ("state", Type::Str),
        ("target", Type::Optional(Box::new(Type::Path))),
        ("errno", Type::Optional(Box::new(Type::Int))),
        ("error_kind", Type::Optional(Box::new(Type::Str))),
    ]))
}

pub fn fs_remove_manifest_result_type() -> Type {
    Type::Record(name_type_map(vec![
        ("removed".to_string(), Type::Int),
        ("missing".to_string(), Type::Int),
        ("pruned_dirs".to_string(), Type::Int),
    ]))
}

pub fn process_entry_type() -> Type {
    Type::Record(name_type_map(vec![
        ("pid".to_string(), Type::Int),
        ("parent_pid".to_string(), Type::Int),
        ("command".to_string(), Type::Str),
        ("argv".to_string(), Type::Str),
        ("argv0".to_string(), Type::Str),
        ("user".to_string(), Type::Str),
        ("uid".to_string(), Type::Int),
        ("status".to_string(), Type::Str),
        ("start_time".to_string(), Type::Str),
        ("start_time_ms".to_string(), Type::Int),
        ("runtime_seconds".to_string(), Type::Int),
        ("user_ticks".to_string(), Type::Optional(Box::new(Type::Int))),
        ("system_ticks".to_string(), Type::Optional(Box::new(Type::Int))),
        ("cpu_ticks".to_string(), Type::Optional(Box::new(Type::Int))),
        ("start_ticks".to_string(), Type::Optional(Box::new(Type::Int))),
        ("ticks_per_second".to_string(), Type::Optional(Box::new(Type::Int))),
        ("rss_bytes".to_string(), Type::Optional(Box::new(Type::Int))),
        ("vsize_bytes".to_string(), Type::Optional(Box::new(Type::Int))),
        ("pgrp".to_string(), Type::Optional(Box::new(Type::Int))),
        ("session".to_string(), Type::Optional(Box::new(Type::Int))),
        ("tty_number".to_string(), Type::Optional(Box::new(Type::Int))),
        ("nice".to_string(), Type::Optional(Box::new(Type::Int))),
        ("priority".to_string(), Type::Optional(Box::new(Type::Int))),
        ("thread_count".to_string(), Type::Optional(Box::new(Type::Int))),
        ("processor".to_string(), Type::Optional(Box::new(Type::Int))),
        ("tty".to_string(), Type::Optional(Box::new(Type::Str))),
    ]))
}

pub fn process_thread_type() -> Type {
    Type::Record(name_type_map(vec![
        ("pid".to_string(), Type::Int),
        ("parent_pid".to_string(), Type::Int),
        ("command".to_string(), Type::Str),
        ("argv".to_string(), Type::Str),
        ("argv0".to_string(), Type::Str),
        ("user".to_string(), Type::Str),
        ("uid".to_string(), Type::Int),
        ("status".to_string(), Type::Str),
        ("start_time".to_string(), Type::Str),
        ("start_time_ms".to_string(), Type::Int),
        ("runtime_seconds".to_string(), Type::Int),
        ("owner_pid".to_string(), Type::Int),
        ("thread_id".to_string(), Type::Int),
        ("thread_name".to_string(), Type::Str),
    ]))
}

pub fn process_stats_type() -> Type {
    Type::Record(name_type_map(vec![
        ("rss_kb".to_string(), Type::Int),
        ("vsz_kb".to_string(), Type::Int),
    ]))
}

pub fn process_port_type() -> Type {
    Type::Record(name_type_map(vec![
        ("pid".to_string(), Type::Int),
        ("parent_pid".to_string(), Type::Int),
        ("command".to_string(), Type::Str),
        ("argv".to_string(), Type::Str),
        ("argv0".to_string(), Type::Str),
        ("user".to_string(), Type::Str),
        ("uid".to_string(), Type::Int),
        ("protocol".to_string(), Type::Str),
        ("local_address".to_string(), Type::Str),
        ("local_port".to_string(), Type::Int),
        ("local".to_string(), Type::Str),
        ("remote_address".to_string(), Type::Str),
        ("remote_port".to_string(), Type::Int),
        ("remote".to_string(), Type::Str),
        ("state".to_string(), Type::Str),
        ("fd".to_string(), Type::Int),
        ("inode".to_string(), Type::Int),
    ]))
}

pub fn signal_record_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("number".to_string(), Type::Int),
    ]))
}

pub fn rlimit_type() -> Type {
    Type::Record(name_type_map(vec![
        ("resource".to_string(), Type::Str),
        ("soft".to_string(), Type::Optional(Box::new(Type::Int))),
        ("hard".to_string(), Type::Optional(Box::new(Type::Int))),
    ]))
}

pub fn process_scheduler_type() -> Type {
    Type::Record(name_type_map(vec![
        ("policy".to_string(), Type::Str),
        ("priority".to_string(), Type::Int),
        ("reset_on_fork".to_string(), Type::Bool),
        ("runtime_ns".to_string(), Type::Int),
        ("deadline_ns".to_string(), Type::Int),
        ("period_ns".to_string(), Type::Int),
    ]))
}

pub fn process_scheduler_range_type() -> Type {
    Type::Record(name_type_map(vec![
        ("min".to_string(), Type::Int),
        ("max".to_string(), Type::Int),
    ]))
}

pub fn process_io_priority_type() -> Type {
    Type::Record(name_type_map(vec![
        ("class".to_string(), Type::Str),
        ("level".to_string(), Type::Int),
    ]))
}

pub fn env_entry_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("value".to_string(), Type::Str),
    ]))
}

pub fn env_raw_entry_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Bytes),
        ("value".to_string(), Type::Bytes),
    ]))
}

pub fn linux_meminfo_type() -> Type {
    Type::Record(name_type_map(vec![
        ("total".to_string(), Type::Int),
        ("free".to_string(), Type::Int),
        ("available".to_string(), Type::Int),
        ("buffers".to_string(), Type::Int),
        ("cached".to_string(), Type::Int),
        ("shared".to_string(), Type::Int),
        ("sreclaimable".to_string(), Type::Int),
        ("swap_total".to_string(), Type::Int),
        ("swap_free".to_string(), Type::Int),
    ]))
}

pub fn system_memory_type() -> Type {
    Type::Record(name_type_map(vec![
        ("total".to_string(), Type::Int),
        ("available".to_string(), Type::Int),
        ("free".to_string(), Type::Int),
        ("swap_total".to_string(), Type::Int),
        ("swap_free".to_string(), Type::Int),
    ]))
}

pub fn system_execution_units_type() -> Type {
    Type::Record(name_type_map(vec![
        ("page_size_bytes".to_string(), Type::Int),
        ("clock_ticks_per_second".to_string(), Type::Int),
    ]))
}

pub fn system_os_release_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("pretty_name".to_string(), Type::Str),
        ("version".to_string(), Type::Str),
        ("version_id".to_string(), Type::Str),
        ("id".to_string(), Type::Str),
    ]))
}

pub fn linux_disk_usage_type() -> Type {
    Type::Record(name_type_map(vec![
        ("device".to_string(), Type::Str),
        ("mount".to_string(), Type::Str),
        ("fstype".to_string(), Type::Str),
        ("total".to_string(), Type::Int),
        ("used".to_string(), Type::Int),
        ("available".to_string(), Type::Int),
    ]))
}

pub fn linux_block_device_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("path".to_string(), Type::Path),
        ("size".to_string(), Type::Int),
        ("sectors".to_string(), Type::Int),
        ("sector_size".to_string(), Type::Int),
        ("removable".to_string(), Type::Bool),
        ("rotational".to_string(), Type::Bool),
        ("partitioned".to_string(), Type::Bool),
        ("partitions".to_string(), Type::List(Box::new(Type::Path))),
    ]))
}

pub fn linux_interface_address_type() -> Type {
    Type::Record(name_type_map(vec![
        ("family".to_string(), Type::Str),
        ("addr".to_string(), Type::Str),
        ("prefix_len".to_string(), Type::Int),
    ]))
}

pub fn linux_interface_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("flags".to_string(), Type::List(Box::new(Type::Str))),
        ("mtu".to_string(), Type::Int),
        ("mac".to_string(), Type::Str),
        (
            "addresses".to_string(),
            Type::List(Box::new(linux_interface_address_type())),
        ),
    ]))
}

pub fn linux_route_type() -> Type {
    Type::Record(name_type_map(vec![
        ("family".to_string(), Type::Str),
        ("dst".to_string(), Type::Str),
        ("prefix_len".to_string(), Type::Int),
        ("gateway".to_string(), Type::Str),
        ("dev".to_string(), Type::Str),
        ("metric".to_string(), Type::Int),
        ("flags".to_string(), Type::List(Box::new(Type::Str))),
    ]))
}

pub fn linux_netlink_attribute_type() -> Type {
    Type::Record(name_type_map(vec![
        ("kind", Type::Int),
        ("data", Type::Bytes),
    ]))
}

pub fn linux_network_link_type() -> Type {
    Type::Record(name_type_map(vec![
        ("ifindex", Type::Int),
        ("name", Type::Optional(Box::new(Type::Str))),
        ("name_bytes", Type::Optional(Box::new(Type::Bytes))),
        ("hardware_type", Type::Int),
        ("flags", Type::Int),
        ("mtu", Type::Optional(Box::new(Type::Int))),
        ("address", Type::Optional(Box::new(Type::Bytes))),
        ("broadcast", Type::Optional(Box::new(Type::Bytes))),
        ("master_ifindex", Type::Optional(Box::new(Type::Int))),
        ("lower_ifindex", Type::Optional(Box::new(Type::Int))),
        ("operstate", Type::Optional(Box::new(Type::Int))),
        ("kind", Type::Optional(Box::new(Type::Str))),
        ("rx_bytes", Type::Optional(Box::new(Type::Int))),
        ("tx_bytes", Type::Optional(Box::new(Type::Int))),
        (
            "attributes",
            Type::List(Box::new(linux_netlink_attribute_type())),
        ),
    ]))
}

pub fn linux_network_address_type() -> Type {
    Type::Record(name_type_map(vec![
        ("ifindex", Type::Int),
        ("family", Type::Str),
        ("prefix_length", Type::Int),
        ("scope", Type::Int),
        ("flags", Type::Int),
        ("address", Type::Optional(Box::new(Type::Str))),
        ("local", Type::Optional(Box::new(Type::Str))),
        ("broadcast", Type::Optional(Box::new(Type::Str))),
        ("label", Type::Optional(Box::new(Type::Str))),
        (
            "preferred_lifetime_seconds",
            Type::Optional(Box::new(Type::Int)),
        ),
        (
            "valid_lifetime_seconds",
            Type::Optional(Box::new(Type::Int)),
        ),
        (
            "attributes",
            Type::List(Box::new(linux_netlink_attribute_type())),
        ),
    ]))
}

pub fn linux_network_nexthop_type() -> Type {
    Type::Record(name_type_map(vec![
        ("ifindex", Type::Int),
        ("flags", Type::Int),
        ("hops", Type::Int),
        ("gateway", Type::Optional(Box::new(Type::Str))),
    ]))
}

pub fn linux_network_route_type() -> Type {
    Type::Record(name_type_map(vec![
        ("family", Type::Str),
        ("destination_prefix_length", Type::Int),
        ("source_prefix_length", Type::Int),
        ("destination", Type::Optional(Box::new(Type::Str))),
        ("source", Type::Optional(Box::new(Type::Str))),
        ("gateway", Type::Optional(Box::new(Type::Str))),
        ("preferred_source", Type::Optional(Box::new(Type::Str))),
        ("output_ifindex", Type::Optional(Box::new(Type::Int))),
        ("input_ifindex", Type::Optional(Box::new(Type::Int))),
        ("table", Type::Int),
        ("priority", Type::Optional(Box::new(Type::Int))),
        ("route_type", Type::Int),
        ("protocol", Type::Int),
        ("scope", Type::Int),
        ("flags", Type::Int),
        (
            "nexthops",
            Type::List(Box::new(linux_network_nexthop_type())),
        ),
        (
            "attributes",
            Type::List(Box::new(linux_netlink_attribute_type())),
        ),
    ]))
}

pub fn linux_network_rule_type() -> Type {
    Type::Record(name_type_map(vec![
        ("family", Type::Str),
        ("destination_prefix_length", Type::Int),
        ("source_prefix_length", Type::Int),
        ("destination", Type::Optional(Box::new(Type::Str))),
        ("source", Type::Optional(Box::new(Type::Str))),
        ("input_name", Type::Optional(Box::new(Type::Str))),
        ("output_name", Type::Optional(Box::new(Type::Str))),
        ("priority", Type::Optional(Box::new(Type::Int))),
        ("table", Type::Int),
        ("fwmark", Type::Optional(Box::new(Type::Int))),
        ("fwmask", Type::Optional(Box::new(Type::Int))),
        ("action", Type::Int),
        ("flags", Type::Int),
        (
            "attributes",
            Type::List(Box::new(linux_netlink_attribute_type())),
        ),
    ]))
}

pub fn linux_network_issue_type() -> Type {
    Type::Record(name_type_map(vec![
        ("object", Type::Str),
        ("message", Type::Str),
        ("state", Type::Str),
        ("errno", Type::Optional(Box::new(Type::Int))),
        ("error_kind", Type::Str),
    ]))
}

pub fn linux_network_dump_type() -> Type {
    Type::Record(name_type_map(vec![
        ("state", Type::Str),
        ("enumeration_succeeded", Type::Bool),
        ("links", Type::List(Box::new(linux_network_link_type()))),
        (
            "addresses",
            Type::List(Box::new(linux_network_address_type())),
        ),
        ("routes", Type::List(Box::new(linux_network_route_type()))),
        ("rules", Type::List(Box::new(linux_network_rule_type()))),
        ("issues", Type::List(Box::new(linux_network_issue_type()))),
    ]))
}

pub fn linux_file_attrs_type() -> Type {
    Type::Record(name_type_map(vec![
        ("flags".to_string(), Type::Int),
        ("indexed_directory".to_string(), Type::Bool),
        ("secure_deletion".to_string(), Type::Bool),
        ("undelete".to_string(), Type::Bool),
        ("sync".to_string(), Type::Bool),
        ("dirsync".to_string(), Type::Bool),
        ("immutable".to_string(), Type::Bool),
        ("append_only".to_string(), Type::Bool),
        ("no_dump".to_string(), Type::Bool),
        ("no_atime".to_string(), Type::Bool),
        ("compression_requested".to_string(), Type::Bool),
        ("journaled_data".to_string(), Type::Bool),
        ("no_tailmerging".to_string(), Type::Bool),
        ("top_of_directory_hierarchies".to_string(), Type::Bool),
    ]))
}

pub fn linux_rfkill_type() -> Type {
    Type::Record(name_type_map(vec![
        ("id".to_string(), Type::Int),
        ("name".to_string(), Type::Str),
        ("type".to_string(), Type::Str),
        ("soft_blocked".to_string(), Type::Bool),
        ("hard_blocked".to_string(), Type::Bool),
    ]))
}

pub fn linux_loop_device_type() -> Type {
    Type::Record(name_type_map(vec![
        ("device".to_string(), Type::Path),
        ("file".to_string(), Type::Path),
        ("offset".to_string(), Type::Int),
        ("size".to_string(), Type::Int),
    ]))
}

pub fn linux_module_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("size".to_string(), Type::Int),
        ("ref_count".to_string(), Type::Int),
        ("used_by".to_string(), Type::List(Box::new(Type::Str))),
    ]))
}

pub fn linux_uevent_type() -> Type {
    Type::Record(name_type_map(vec![
        ("action".to_string(), Type::Str),
        ("subsystem".to_string(), Type::Str),
        ("devname".to_string(), Type::Str),
        ("devpath".to_string(), Type::Str),
        ("env".to_string(), Type::List(Box::new(env_entry_type()))),
    ]))
}

pub fn linux_blkid_type() -> Type {
    Type::Record(name_type_map(vec![
        ("type".to_string(), Type::Str),
        ("uuid".to_string(), Type::Str),
        ("label".to_string(), Type::Str),
        ("part_table_type".to_string(), Type::Str),
        ("part_entry_uuid".to_string(), Type::Str),
    ]))
}

pub fn linux_module_param_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("type".to_string(), Type::Str),
        ("description".to_string(), Type::Str),
    ]))
}

pub fn linux_modinfo_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("filename".to_string(), Type::Path),
        ("description".to_string(), Type::Str),
        ("license".to_string(), Type::Str),
        ("version".to_string(), Type::Str),
        (
            "params".to_string(),
            Type::List(Box::new(linux_module_param_type())),
        ),
        ("fields".to_string(), Type::List(Box::new(env_entry_type()))),
    ]))
}

pub fn linux_open_file_type() -> Type {
    Type::Record(name_type_map(vec![
        ("pid".to_string(), Type::Int),
        ("command".to_string(), Type::Str),
        ("fd".to_string(), Type::Int),
        ("type".to_string(), Type::Str),
        ("path".to_string(), Type::Path),
        ("inode".to_string(), Type::Int),
        ("protocol".to_string(), Type::Str),
        ("local".to_string(), Type::Str),
        ("remote".to_string(), Type::Str),
        ("fd_label".to_string(), Type::Str),
        ("access".to_string(), Type::Str),
        ("dev".to_string(), Type::Optional(Box::new(Type::Int))),
    ]))
}

pub fn linux_partition_type() -> Type {
    Type::Record(name_type_map(vec![
        ("index".to_string(), Type::Int),
        ("start".to_string(), Type::Int),
        ("end".to_string(), Type::Int),
        ("size".to_string(), Type::Int),
        ("type".to_string(), Type::Str),
        ("uuid".to_string(), Type::Str),
        ("name".to_string(), Type::Str),
    ]))
}

pub fn linux_partition_table_type() -> Type {
    Type::Record(name_type_map(vec![
        ("label".to_string(), Type::Str),
        ("id".to_string(), Type::Str),
        ("sector_size".to_string(), Type::Int),
        (
            "partitions".to_string(),
            Type::List(Box::new(linux_partition_type())),
        ),
    ]))
}

pub fn linux_fsck_type() -> Type {
    Type::Record(name_type_map(vec![
        ("status".to_string(), Type::Int),
        ("errors".to_string(), Type::List(Box::new(Type::Str))),
    ]))
}

pub fn unix_kill_all_result_type() -> Type {
    Type::Record(name_type_map(vec![
        ("matched".to_string(), Type::Int),
        ("signaled".to_string(), Type::Int),
    ]))
}

pub fn unix_group_id_type() -> Type {
    Type::Record(name_type_map(vec![
        ("gid".to_string(), Type::Int),
        ("name".to_string(), Type::Str),
    ]))
}

pub fn unix_id_type() -> Type {
    Type::Record(name_type_map(vec![
        ("uid".to_string(), Type::Int),
        ("euid".to_string(), Type::Int),
        ("gid".to_string(), Type::Int),
        ("egid".to_string(), Type::Int),
        (
            "groups".to_string(),
            Type::List(Box::new(unix_group_id_type())),
        ),
        (
            "supplementary".to_string(),
            Type::List(Box::new(Type::Int)),
        ),
    ]))
}

pub fn unix_tty_attrs_type() -> Type {
    Type::Record(name_type_map(vec![
        ("iflag".to_string(), Type::Int),
        ("oflag".to_string(), Type::Int),
        ("cflag".to_string(), Type::Int),
        ("lflag".to_string(), Type::Int),
        ("line".to_string(), Type::Int),
        ("ispeed".to_string(), Type::Int),
        ("ospeed".to_string(), Type::Int),
        ("echo".to_string(), Type::Bool),
        ("raw".to_string(), Type::Bool),
        ("crnl".to_string(), Type::Bool),
        ("control_chars".to_string(), Type::List(Box::new(Type::Int))),
    ]))
}

pub fn unix_tty_flag_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("field".to_string(), Type::Str),
        ("mask".to_string(), Type::Int),
        ("value".to_string(), Type::Int),
        ("sane".to_string(), Type::Bool),
    ]))
}

pub fn unix_tty_char_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("index".to_string(), Type::Int),
        ("sane".to_string(), Type::Int),
    ]))
}

pub fn unix_tty_table_type() -> Type {
    Type::Record(name_type_map(vec![
        (
            "flags".to_string(),
            Type::List(Box::new(unix_tty_flag_type())),
        ),
        (
            "chars".to_string(),
            Type::List(Box::new(unix_tty_char_type())),
        ),
        ("speeds".to_string(), Type::List(Box::new(Type::Int))),
    ]))
}

pub fn unix_window_size_type() -> Type {
    Type::Record(name_type_map(vec![
        ("rows".to_string(), Type::Int),
        ("cols".to_string(), Type::Int),
        ("xpixel".to_string(), Type::Int),
        ("ypixel".to_string(), Type::Int),
    ]))
}

pub fn unix_pty_type() -> Type {
    Type::Record(name_type_map(vec![
        ("master".to_string(), Type::Int),
        ("replica".to_string(), Type::Int),
        ("name".to_string(), Type::Str),
    ]))
}

pub fn unix_utmp_type() -> Type {
    Type::Record(name_type_map(vec![
        ("type".to_string(), Type::Int),
        ("kind".to_string(), Type::Str),
        ("pid".to_string(), Type::Int),
        ("line".to_string(), Type::Str),
        ("id".to_string(), Type::Str),
        ("user".to_string(), Type::Str),
        ("host".to_string(), Type::Str),
        ("termination".to_string(), Type::Int),
        ("exit_status".to_string(), Type::Int),
        ("session".to_string(), Type::Int),
        ("time_sec".to_string(), Type::Int),
        ("time_usec".to_string(), Type::Int),
        ("addr".to_string(), Type::Str),
    ]))
}

pub fn unix_load_average_type() -> Type {
    Type::Record(name_type_map(vec![
        ("one".to_string(), Type::Float),
        ("five".to_string(), Type::Float),
        ("fifteen".to_string(), Type::Float),
    ]))
}

pub fn spawn_record_type() -> Type {
    Type::Record(name_type_map(vec![
        ("pid".to_string(), Type::Int),
        ("command".to_string(), Type::Str),
        ("argv".to_string(), Type::Str),
        ("detach".to_string(), Type::Bool),
        ("new_session".to_string(), Type::Bool),
        ("ignore_hup".to_string(), Type::Bool),
    ]))
}

pub fn process_wait_any_type() -> Type {
    Type::Record(name_type_map(vec![
        ("index".to_string(), Type::Int),
        ("pid".to_string(), Type::Int),
        ("status".to_string(), Type::Status),
    ]))
}

pub fn unix_child_event_type() -> Type {
    Type::Record(name_type_map(vec![
        ("pid".to_string(), Type::Int),
        ("status".to_string(), Type::Status),
    ]))
}

pub fn unix_pid1_event_type() -> Type {
    Type::Record(name_type_map(vec![
        ("kind".to_string(), Type::Str),
        ("signal".to_string(), Type::Str),
        (
            "children".to_string(),
            Type::List(Box::new(unix_child_event_type())),
        ),
    ]))
}

pub fn unix_pid1_shutdown_type() -> Type {
    Type::Record(name_type_map(vec![
        ("term_sent".to_string(), Type::Int),
        ("kill_sent".to_string(), Type::Int),
        (
            "reaped".to_string(),
            Type::List(Box::new(unix_child_event_type())),
        ),
        ("remaining".to_string(), Type::List(Box::new(Type::Int))),
    ]))
}

pub fn unix_spawned_child_type() -> Type {
    Type::Record(name_type_map(vec![
        ("pid".to_string(), Type::Int),
        ("command".to_string(), Type::Str),
        ("argv".to_string(), Type::List(Box::new(Type::Str))),
        ("detach".to_string(), Type::Bool),
        ("new_session".to_string(), Type::Bool),
        ("ignore_hup".to_string(), Type::Bool),
        ("notify_fd".to_string(), Type::Int),
    ]))
}

pub fn unix_logged_process_group_type() -> Type {
    Type::Record(name_type_map(vec![
        ("pid".to_string(), Type::Int),
        ("log_pid".to_string(), Type::Int),
        ("command".to_string(), Type::Str),
        ("argv".to_string(), Type::List(Box::new(Type::Str))),
        ("detach".to_string(), Type::Bool),
        ("new_session".to_string(), Type::Bool),
        ("ignore_hup".to_string(), Type::Bool),
    ]))
}

pub fn measured_command_type() -> Type {
    Type::Record(name_type_map(vec![
        ("status".to_string(), Type::Status),
        ("duration_ms".to_string(), Type::Int),
        ("wall_ns".to_string(), Type::Int),
        ("user_ns".to_string(), Type::Int),
        ("system_ns".to_string(), Type::Int),
    ]))
}

#[cfg(feature = "native-tests")]
pub fn test_context_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("file".to_string(), Type::Path),
        ("temp_root".to_string(), Type::Path),
        ("core_dir".to_string(), Type::Path),
        ("xsh_bin".to_string(), Type::Path),
    ]))
}

#[cfg(feature = "native-tests")]
pub fn test_call_type() -> Type {
    Type::Record(name_type_map(vec![
        ("op".to_string(), Type::Str),
        ("args".to_string(), Type::Record(BTreeMap::new())),
    ]))
}

#[cfg(feature = "native-tests")]
pub fn test_script_output_type() -> Type {
    Type::Record(name_type_map(vec![
        ("success".to_string(), Type::Bool),
        ("status".to_string(), Type::Int),
        ("stdout".to_string(), Type::Str),
        ("stderr".to_string(), Type::Str),
        ("stdout_bytes".to_string(), Type::Bytes),
        ("stderr_bytes".to_string(), Type::Bytes),
    ]))
}

pub fn uname_record_type() -> Type {
    Type::Record(name_type_map(vec![
        ("sysname".to_string(), Type::Str),
        ("nodename".to_string(), Type::Str),
        ("release".to_string(), Type::Str),
        ("version".to_string(), Type::Str),
        ("machine".to_string(), Type::Str),
    ]))
}

pub fn user_record_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("uid".to_string(), Type::Int),
        ("gid".to_string(), Type::Int),
        ("home".to_string(), Type::Path),
        ("shell".to_string(), Type::Str),
    ]))
}

pub fn group_record_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("gid".to_string(), Type::Int),
        ("members".to_string(), Type::List(Box::new(Type::Str))),
    ]))
}

pub fn linux_module_plan_type() -> Type {
    Type::Record(name_type_map(vec![("name", Type::Str), ("filename", Type::Path), ("params", Type::Str), ("loaded", Type::Bool)]))
}

/// Capability numbers are lists of `Int`, ascending; `securebits` holds flag names.
pub fn linux_privileges_type() -> Type {
    let numbers = || Type::List(Box::new(Type::Int));
    Type::Record(name_type_map(vec![
        ("effective", numbers()),
        ("permitted", numbers()),
        ("inheritable", numbers()),
        ("bounding", numbers()),
        ("ambient", numbers()),
        ("securebits", Type::List(Box::new(Type::Str))),
        ("no_new_privs", Type::Bool),
        ("parent_death_signal", Type::Int),
        ("last_capability", Type::Int),
    ]))
}

pub fn linux_namespace_type() -> Type {
    Type::Record(name_type_map(vec![
        ("ns", Type::Int),
        ("type", Type::Str),
        ("path", Type::Path),
        ("nprocs", Type::Int),
        ("pid", Type::Int),
        ("ppid", Type::Int),
        ("uid", Type::Int),
        ("command", Type::Str),
        ("pns", Type::Int),
        ("ons", Type::Int),
        ("netnsid", Type::Optional(Box::new(Type::Int))),
        ("nsfs", Type::List(Box::new(Type::Path))),
    ]))
}

pub fn linux_blockdev_info_type() -> Type {
    Type::Record(name_type_map(vec![("size_bytes", Type::UInt), ("logical_sector_bytes", Type::UInt), ("physical_sector_bytes", Type::UInt), ("read_only", Type::Bool)]))
}

pub fn time_calendar_type() -> Type {
    Type::Record(name_type_map(["year", "month", "day", "hour", "minute", "second", "weekday", "offset_seconds", "nanosecond"].into_iter().map(|name| (name, Type::Int)).collect()))
}
