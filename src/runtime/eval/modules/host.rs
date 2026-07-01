use crate::modules::{group, unix, user};
use crate::runtime::process::ProcessInvocation;
use crate::runtime::value::{PathValue, RecordMap, RuntimeError, Value};
use crate::source::{SourceId, Span};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostError {
    pub kind: String,
    pub message: String,
}

impl HostError {
    fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }

    #[allow(clippy::single_call_fn)]
    fn from_error_value(value: Value) -> Self {
        match value {
            Value::Error(error) => (*error).into(),
            Value::RunError(error) => Self::new(error.kind, error.message),
            value => Self::new(
                "host-error",
                format!("expected error value, found {}", value.type_name()),
            ),
        }
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for HostError {}

impl From<RuntimeError> for HostError {
    fn from(error: RuntimeError) -> Self {
        Self::new(error.kind, error.message)
    }
}

pub type HostResult<T> = Result<T, HostError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserRecord {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub home: PathBuf,
    pub shell: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupRecord {
    pub name: String,
    pub gid: u32,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtyAttrs {
    pub iflag: u64,
    pub oflag: u64,
    pub cflag: u64,
    pub lflag: u64,
    pub ispeed: u64,
    pub ospeed: u64,
    pub echo: bool,
    pub raw: bool,
    pub crnl: bool,
    pub control_chars: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpawnedChild {
    pub pid: i64,
    pub command: String,
    pub argv: Vec<String>,
    pub detach: bool,
    pub new_session: bool,
    pub ignore_hup: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub target: OsString,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(OsString, OsString)>,
    pub clear_env: bool,
    pub timeout: Option<Duration>,
}

impl CommandSpec {
    pub fn new(target: impl Into<OsString>) -> Self {
        Self {
            target: target.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            clear_env: false,
            timeout: None,
        }
    }

    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn env(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((name.into(), value.into()));
        self
    }

    pub fn clear_env(mut self) -> Self {
        self.clear_env = true;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

pub fn lookup_user(name: &str) -> HostResult<UserRecord> {
    user_value(user::lookup(name, host_span())?)
}

pub fn user_by_uid(uid: u32) -> HostResult<UserRecord> {
    user_value(user::by_uid(uid, host_span())?)
}

pub fn lookup_group(name: &str) -> HostResult<GroupRecord> {
    group_value(group::lookup(name, host_span())?)
}

pub fn tty_attrs(fd: i32) -> HostResult<TtyAttrs> {
    let value = unwrap_result_value(unix::tty_attrs(i64::from(fd), host_span())?)?;
    tty_attrs_value(value)
}

pub fn set_tty_attrs(fd: i32, attrs: &TtyAttrs) -> HostResult<()> {
    let record = tty_attrs_record(attrs);
    let value = unwrap_result_value(unix::set_tty_attrs(&record, i64::from(fd), host_span())?)?;
    unit_value(value)
}

pub fn spawn_with_tty(command: &CommandSpec, tty: &str) -> HostResult<SpawnedChild> {
    let invocation = command_invocation(command)?;
    let value = unwrap_result_value(unix::spawn_with_tty(&invocation, tty, host_span())?)?;
    spawned_child_value(value)
}

pub fn exec(command: &CommandSpec) -> HostResult<()> {
    let invocation = command_invocation(command)?;
    let value = unwrap_result_value(unix::exec(&invocation, host_span())?)?;
    unit_value(value)
}

fn command_invocation(command: &CommandSpec) -> HostResult<ProcessInvocation> {
    let mut env = if command.clear_env {
        BTreeMap::new()
    } else {
        let mut env = BTreeMap::new();
        for (name, value) in std::env::vars_os() {
            env.insert(os_bytes(&name)?, os_bytes(&value)?);
        }
        env
    };
    let mut env_overlay = BTreeMap::new();
    for (name, value) in &command.env {
        let name_bytes = os_bytes(name)?;
        let value_bytes = os_bytes(value)?;
        if name_bytes.is_empty() || name_bytes.contains(&b'=') {
            return Err(HostError::new(
                "env-name",
                "environment names cannot be empty or contain `=`",
            ));
        }
        env.insert(name_bytes.clone(), value_bytes.clone());
        env_overlay.insert(name_bytes, value_bytes);
    }

    Ok(ProcessInvocation {
        target: os_bytes(&command.target)?,
        argv: command
            .args
            .iter()
            .map(|arg| os_bytes(arg))
            .collect::<HostResult<Vec<_>>>()?,
        cwd: match &command.cwd {
            Some(cwd) => cwd.clone(),
            None => {
                std::env::current_dir().map_err(|error| HostError::new("cwd", error.to_string()))?
            }
        },
        env,
        env_overlay,
        redirections: Vec::new(),
        timeout: command.timeout,
        cpu_max: None,
    })
}

fn os_bytes(value: &OsStr) -> HostResult<Vec<u8>> {
    let bytes = value.as_bytes();
    if bytes.contains(&0) {
        Err(HostError::new("nul-byte", "OS strings cannot contain NUL"))
    } else {
        Ok(bytes.to_vec())
    }
}

fn unwrap_result_value(value: Value) -> HostResult<Value> {
    match value {
        Value::Result(result) => match result {
            crate::runtime::value::ResultValue::Ok(value) => Ok(*value),
            crate::runtime::value::ResultValue::Err(error) => {
                Err(HostError::from_error_value(*error))
            }
        },
        value => Ok(value),
    }
}

#[allow(clippy::single_call_fn)]
fn user_value(value: Value) -> HostResult<UserRecord> {
    let record = record_value(value)?;
    Ok(UserRecord {
        name: str_field(&record, "name")?,
        uid: u32_field(&record, "uid")?,
        gid: u32_field(&record, "gid")?,
        home: path_field(&record, "home")?,
        shell: str_field(&record, "shell")?,
    })
}

#[allow(clippy::single_call_fn)]
fn group_value(value: Value) -> HostResult<GroupRecord> {
    let record = record_value(value)?;
    Ok(GroupRecord {
        name: str_field(&record, "name")?,
        gid: u32_field(&record, "gid")?,
        members: str_list_field(&record, "members")?,
    })
}

#[allow(clippy::single_call_fn)]
fn tty_attrs_value(value: Value) -> HostResult<TtyAttrs> {
    let record = record_value(value)?;
    Ok(TtyAttrs {
        iflag: u64_field(&record, "iflag")?,
        oflag: u64_field(&record, "oflag")?,
        cflag: u64_field(&record, "cflag")?,
        lflag: u64_field(&record, "lflag")?,
        ispeed: u64_field(&record, "ispeed")?,
        ospeed: u64_field(&record, "ospeed")?,
        echo: bool_field(&record, "echo")?,
        raw: bool_field(&record, "raw")?,
        crnl: bool_field(&record, "crnl")?,
        control_chars: u8_list_field(&record, "control_chars")?,
    })
}

#[allow(clippy::single_call_fn)]
fn tty_attrs_record(attrs: &TtyAttrs) -> RecordMap {
    use std::sync::Arc;
    RecordMap::from([
        (Arc::from("iflag"), Value::Int(attrs.iflag as i64)),
        (Arc::from("oflag"), Value::Int(attrs.oflag as i64)),
        (Arc::from("cflag"), Value::Int(attrs.cflag as i64)),
        (Arc::from("lflag"), Value::Int(attrs.lflag as i64)),
        (Arc::from("ispeed"), Value::Int(attrs.ispeed as i64)),
        (Arc::from("ospeed"), Value::Int(attrs.ospeed as i64)),
        (Arc::from("echo"), Value::Bool(attrs.echo)),
        (Arc::from("raw"), Value::Bool(attrs.raw)),
        (Arc::from("crnl"), Value::Bool(attrs.crnl)),
        (
            Arc::from("control_chars"),
            Value::List(
                attrs
                    .control_chars
                    .iter()
                    .map(|value| Value::Int(i64::from(*value)))
                    .collect(),
            ),
        ),
    ])
}

#[allow(clippy::single_call_fn)]
fn spawned_child_value(value: Value) -> HostResult<SpawnedChild> {
    let record = record_value(value)?;
    Ok(SpawnedChild {
        pid: int_field(&record, "pid")?,
        command: str_field(&record, "command")?,
        argv: str_list_field(&record, "argv")?,
        detach: bool_field(&record, "detach")?,
        new_session: bool_field(&record, "new_session")?,
        ignore_hup: bool_field(&record, "ignore_hup")?,
    })
}

fn unit_value(value: Value) -> HostResult<()> {
    match value {
        Value::Unit => Ok(()),
        value => Err(HostError::new(
            "type-error",
            format!("expected Unit, found {}", value.type_name()),
        )),
    }
}

fn record_value(value: Value) -> HostResult<RecordMap> {
    match value {
        Value::Record(record) => Ok(record),
        value => Err(HostError::new(
            "type-error",
            format!("expected Record, found {}", value.type_name()),
        )),
    }
}

fn str_field(record: &RecordMap, name: &str) -> HostResult<String> {
    match record.get(name) {
        Some(Value::Str(value)) => Ok(value.to_string()),
        Some(value) => Err(field_type_error(name, "Str", value)),
        None => Err(missing_field_error(name)),
    }
}

fn str_list_field(record: &RecordMap, name: &str) -> HostResult<Vec<String>> {
    let Some(Value::List(values)) = record.get(name) else {
        return match record.get(name) {
            Some(value) => Err(field_type_error(name, "List[Str]", value)),
            None => Err(missing_field_error(name)),
        };
    };
    values
        .iter()
        .map(|value| match value {
            Value::Str(value) => Ok(value.to_string()),
            value => Err(field_type_error(name, "List[Str]", value)),
        })
        .collect()
}

#[allow(clippy::single_call_fn)]
fn path_field(record: &RecordMap, name: &str) -> HostResult<PathBuf> {
    match record.get(name) {
        Some(Value::Path(PathValue { bytes })) => {
            Ok(PathBuf::from(OsString::from_vec(bytes.clone())))
        }
        Some(value) => Err(field_type_error(name, "Path", value)),
        None => Err(missing_field_error(name)),
    }
}

fn int_field(record: &RecordMap, name: &str) -> HostResult<i64> {
    match record.get(name) {
        Some(Value::Int(value)) => Ok(*value),
        Some(value) => Err(field_type_error(name, "Int", value)),
        None => Err(missing_field_error(name)),
    }
}

fn u32_field(record: &RecordMap, name: &str) -> HostResult<u32> {
    let value = int_field(record, name)?;
    u32::try_from(value).map_err(|_| HostError::new("range", format!("{name} is out of range")))
}

fn u64_field(record: &RecordMap, name: &str) -> HostResult<u64> {
    let value = int_field(record, name)?;
    u64::try_from(value).map_err(|_| HostError::new("range", format!("{name} is out of range")))
}

fn bool_field(record: &RecordMap, name: &str) -> HostResult<bool> {
    match record.get(name) {
        Some(Value::Bool(value)) => Ok(*value),
        Some(value) => Err(field_type_error(name, "Bool", value)),
        None => Err(missing_field_error(name)),
    }
}

#[allow(clippy::single_call_fn)]
fn u8_list_field(record: &RecordMap, name: &str) -> HostResult<Vec<u8>> {
    let Some(Value::List(values)) = record.get(name) else {
        return match record.get(name) {
            Some(value) => Err(field_type_error(name, "List[Int]", value)),
            None => Err(missing_field_error(name)),
        };
    };
    values
        .iter()
        .map(|value| match value {
            Value::Int(value) => u8::try_from(*value)
                .map_err(|_| HostError::new("range", format!("{name} contains out-of-range Int"))),
            value => Err(field_type_error(name, "List[Int]", value)),
        })
        .collect()
}

fn missing_field_error(name: &str) -> HostError {
    HostError::new("type-error", format!("missing `{name}` field"))
}

fn field_type_error(name: &str, expected: &str, value: &Value) -> HostError {
    HostError::new(
        "type-error",
        format!("{name} expected {expected}, found {}", value.type_name()),
    )
}

fn host_span() -> Span {
    Span::new(SourceId::new(0), 0, 0)
}

#[cfg(test)]
mod tests {
    use super::{
        CommandSpec, Duration, OsString, OsStringExt, PathBuf, TtyAttrs, Value, command_invocation,
        tty_attrs_record, tty_attrs_value,
    };

    #[test]
    fn command_spec_builds_process_invocation() {
        let command = CommandSpec::new("/bin/login")
            .arg("-f")
            .arg("root")
            .cwd("/tmp")
            .clear_env()
            .env("TERM", "xterm-256color")
            .timeout(Duration::from_secs(5));

        let invocation = command_invocation(&command).expect("invocation");

        assert_eq!(invocation.target, b"/bin/login");
        assert_eq!(invocation.argv, vec![b"-f".to_vec(), b"root".to_vec()]);
        assert_eq!(invocation.cwd, PathBuf::from("/tmp"));
        assert_eq!(
            invocation.env.get(b"TERM".as_slice()),
            Some(&b"xterm-256color".to_vec())
        );
        assert_eq!(
            invocation.env_overlay.get(b"TERM".as_slice()),
            Some(&b"xterm-256color".to_vec())
        );
        assert_eq!(invocation.env.len(), 1);
        assert_eq!(invocation.timeout, Some(Duration::from_secs(5)));
    }

    #[test]
    fn command_spec_rejects_invalid_os_strings() {
        let bad_target = CommandSpec::new(OsString::from_vec(b"/bin/login\0".to_vec()));
        let error = command_invocation(&bad_target).expect_err("target NUL rejected");
        assert_eq!(error.kind, "nul-byte");

        let bad_env = CommandSpec::new("/bin/login")
            .clear_env()
            .env("BAD=NAME", "value");
        let error = command_invocation(&bad_env).expect_err("env name rejected");
        assert_eq!(error.kind, "env-name");
    }

    #[test]
    fn tty_attrs_record_roundtrips_through_typed_facade() {
        let attrs = TtyAttrs {
            iflag: 1,
            oflag: 2,
            cflag: 3,
            lflag: 4,
            ispeed: 9600,
            ospeed: 9600,
            echo: true,
            raw: false,
            crnl: true,
            control_chars: vec![3, 4, 127],
        };

        let roundtrip = tty_attrs_value(Value::Record(tty_attrs_record(&attrs))).expect("attrs");

        assert_eq!(roundtrip, attrs);
    }
}
