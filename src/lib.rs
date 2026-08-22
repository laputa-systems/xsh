extern crate self as xsh;

pub mod api {
    //! Read-only language API metadata used by tooling.

    pub use super::modules::signature::{ApiDocs, MethodReturn, ModuleSig, ParamSig};
    pub use super::modules::{ApiArgCheck, MethodReceiver, MethodSig, ModuleFnSig, api_spec};
}
/// Supported structured and rendered diagnostics.
pub mod diagnostic;
pub mod execution;
/// Tooling-only frontend statistics support for the dedicated profiling binary.
pub mod frontend_stats;
#[path = "runtime/eval/modules/host.rs"]
mod host_impl;
pub mod host {
    //! Narrow reusable host adapters; XSH standard-module implementations stay internal.

    pub use super::host_impl::{
        CommandSpec, GroupRecord, HostError, HostResult, SpawnedChild, TtyAttrs, UserRecord, exec,
        lookup_group, lookup_user, set_tty_attrs, spawn_with_tty, tty_attrs, user_by_uid,
    };

    pub mod fs {
        pub use crate::modules::fs::gitroot;
    }

    pub mod ini {
        pub use crate::modules::ini::decode;
    }

    pub mod json {
        pub use crate::modules::json::{
            compact_raw_json, parse_raw_json, pretty_raw_json, raw_json_array, raw_json_as_bool,
            raw_json_as_str, raw_json_as_u64, raw_json_bool, raw_json_f64, raw_json_get,
            raw_json_i64, raw_json_object, raw_json_string, raw_json_u64, raw_json_usize,
        };
    }
}
pub mod frontend;
pub(crate) mod loader;
/// Tooling-only allocation counters used by `xsh-frontend-stats`.
pub mod mem_track;
pub(crate) mod modules;
pub mod process;
pub(crate) mod runner;
pub(crate) mod runtime;
/// Tooling-only runtime allocation accounting used by `xsh-runtime-stats`.
pub mod runtime_stats;
pub(crate) mod sema;
pub(crate) mod source;
pub(crate) mod symbol;
pub(crate) mod syntax;
pub(crate) mod terminal;
pub mod trace;
