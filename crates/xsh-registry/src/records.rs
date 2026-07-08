use crate::types::Type;
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
        ("EnvPathEntry", env_path_entry_type()),
        ("FsCopyTreeResult", fs_copy_tree_result_type()),
        ("FsEntry", fs_entry_type()),
        ("FsFilesystemStats", fs_filesystem_stats_type()),
        ("FsLock", fs_lock_type()),
        ("FsMount", fs_mount_type()),
        ("FsRemoveManifestResult", fs_remove_manifest_result_type()),
        ("FsRoot", fs_root_type()),
        ("Group", group_record_type()),
        ("LinuxBlockDevice", linux_block_device_type()),
        ("LinuxDiskUsage", linux_disk_usage_type()),
        ("LinuxFileAttrs", linux_file_attrs_type()),
        ("LinuxInterface", linux_interface_type()),
        ("LinuxInterfaceAddress", linux_interface_address_type()),
        ("LinuxRoute", linux_route_type()),
        ("LinuxLoopDevice", linux_loop_device_type()),
        ("LinuxMemInfo", linux_meminfo_type()),
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
        ("MimeInfo", mime_info_type()),
        ("MimeParse", mime_parse_type()),
        ("NetHeader", net_header_type()),
        ("NetPool", net_pool_type()),
        ("NetResponse", net_response_type(true)),
        ("PatchResult", patch_result_type()),
        ("ProcessEntry", process_entry_type()),
        ("ProcessPort", process_port_type()),
        ("ProcessThread", process_thread_type()),
        ("Signal", signal_record_type()),
        ("Spawn", spawn_record_type()),
        ("SystemMemory", system_memory_type()),
        ("SystemOsRelease", system_os_release_type()),
        ("TestCall", test_call_type()),
        ("TestContext", test_context_type()),
        ("Uname", uname_record_type()),
        ("UnixChildEvent", unix_child_event_type()),
        ("UnixGroupId", unix_group_id_type()),
        ("UnixId", unix_id_type()),
        ("UnixKillAllResult", unix_kill_all_result_type()),
        ("UnixLoggedProcessGroup", unix_logged_process_group_type()),
        ("UnixPid1Event", unix_pid1_event_type()),
        ("UnixPid1Shutdown", unix_pid1_shutdown_type()),
        ("UnixSpawnedChild", unix_spawned_child_type()),
        ("UnixTtyAttrs", unix_tty_attrs_type()),
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

pub fn fs_mount_type() -> Type {
    Type::Record(name_type_map(vec![
        ("filesystem".to_string(), Type::Str),
        ("mounted_on".to_string(), Type::Path),
        ("fstype".to_string(), Type::Str),
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

pub fn mime_info_type() -> Type {
    Type::Record(name_type_map(vec![
        ("mime".to_string(), Type::Str),
        ("exts".to_string(), Type::List(Box::new(Type::Str))),
    ]))
}

pub fn mime_parse_type() -> Type {
    Type::Record(name_type_map(vec![
        ("type".to_string(), Type::Str),
        ("params".to_string(), Type::Map(Box::new(Type::Str))),
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
    Type::Record(name_type_map(vec![
        ("id".to_string(), Type::Int),
        ("path".to_string(), Type::Path),
        ("shared".to_string(), Type::Bool),
    ]))
}

pub fn fs_root_type() -> Type {
    Type::Record(name_type_map(vec![("id".to_string(), Type::Int)]))
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

pub fn env_entry_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("value".to_string(), Type::Str),
    ]))
}

pub fn linux_meminfo_type() -> Type {
    Type::Record(name_type_map(vec![
        ("total".to_string(), Type::Int),
        ("free".to_string(), Type::Int),
        ("available".to_string(), Type::Int),
        ("buffers".to_string(), Type::Int),
        ("cached".to_string(), Type::Int),
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
    ]))
}

pub fn unix_tty_attrs_type() -> Type {
    Type::Record(name_type_map(vec![
        ("iflag".to_string(), Type::Int),
        ("oflag".to_string(), Type::Int),
        ("cflag".to_string(), Type::Int),
        ("lflag".to_string(), Type::Int),
        ("ispeed".to_string(), Type::Int),
        ("ospeed".to_string(), Type::Int),
        ("echo".to_string(), Type::Bool),
        ("raw".to_string(), Type::Bool),
        ("crnl".to_string(), Type::Bool),
        ("control_chars".to_string(), Type::List(Box::new(Type::Int))),
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

pub fn test_context_type() -> Type {
    Type::Record(name_type_map(vec![
        ("name".to_string(), Type::Str),
        ("file".to_string(), Type::Path),
        ("temp_root".to_string(), Type::Path),
    ]))
}

pub fn test_call_type() -> Type {
    Type::Record(name_type_map(vec![
        ("op".to_string(), Type::Str),
        ("args".to_string(), Type::Record(BTreeMap::new())),
    ]))
}

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
