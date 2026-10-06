use crate::types::Type;
use std::collections::BTreeMap;

fn record(fields: impl IntoIterator<Item = (&'static str, Type)>) -> Type {
    Type::Record(fields.into_iter().map(|(name, ty)| (name.to_string(), ty)).collect())
}

pub fn linux_cpu_sample_type() -> Type {
    record(["user", "nice", "system", "idle", "iowait", "irq", "softirq", "steal"]
        .map(|name| (name, Type::Int)))
}

pub fn linux_disk_sample_type() -> Type {
    let mut fields = ["major", "minor", "reads_completed", "reads_merged", "sectors_read",
        "read_ms", "writes_completed", "writes_merged", "sectors_written", "write_ms",
        "in_flight", "io_ms", "weighted_io_ms"].map(|name| (name.to_string(), Type::Int))
        .into_iter().collect::<BTreeMap<_, _>>();
    fields.insert("name".to_string(), Type::Str);
    Type::Record(fields)
}

pub fn linux_process_sample_type() -> Type {
    let Type::Record(mut fields) = super::process_entry_type() else {
        unreachable!("process inventory schema is a record");
    };
    for name in ["user_ticks", "system_ticks", "cpu_ticks", "start_ticks", "ticks_per_second",
        "rss_bytes", "vsize_bytes", "pgrp", "session", "tty_number", "nice", "priority",
        "thread_count", "processor"] {
        fields.insert(name.to_string(), Type::Int);
    }
    fields.insert("tty".to_string(), Type::Str);
    Type::Record(fields)
}

pub fn linux_sample_type() -> Type {
    let mut fields = ["sampled_at_ms", "uptime_ms", "ticks_per_second", "page_size",
        "context_switches", "processes_created", "running", "blocked", "interrupts",
        "page_in_kib", "page_out_kib", "swap_in_pages", "swap_out_pages"]
        .map(|name| (name.to_string(), Type::Int)).into_iter().collect::<BTreeMap<_, _>>();
    fields.insert("cpu".to_string(), linux_cpu_sample_type());
    fields.insert("memory".to_string(), super::linux_meminfo_type());
    fields.insert("processes".to_string(), Type::List(Box::new(linux_process_sample_type())));
    fields.insert("disks".to_string(), Type::List(Box::new(linux_disk_sample_type())));
    Type::Record(fields)
}
