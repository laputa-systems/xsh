#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunFormReference {
    pub form: &'static str,
    pub context: Option<&'static str>,
    pub returns: &'static str,
    pub nonzero_exit: &'static str,
    pub failure: &'static str,
}

pub const RUN_FORM_REFERENCES: &[RunFormReference] = &[
    RunFormReference {
        form: "run",
        context: Some("statement position"),
        returns: "Unit",
        nonzero_exit: "propagates ProcessError",
        failure: "propagates ProcessError",
    },
    RunFormReference {
        form: "run",
        context: Some("value position"),
        returns: "Status",
        nonzero_exit: "status data",
        failure: "propagates ProcessError",
    },
    RunFormReference {
        form: "run.status",
        context: None,
        returns: "Status",
        nonzero_exit: "status data",
        failure: "propagates ProcessError",
    },
    RunFormReference {
        form: "run.text",
        context: None,
        returns: "Result[Str, ProcessError]",
        nonzero_exit: "Err(ProcessError)",
        failure: "Err(ProcessError)",
    },
    RunFormReference {
        form: "run.bytes",
        context: None,
        returns: "Result[Bytes, ProcessError]",
        nonzero_exit: "Err(ProcessError)",
        failure: "Err(ProcessError)",
    },
    RunFormReference {
        form: "run.capture --text",
        context: None,
        returns: "Result[{status, stdout: Str, stderr: Str}, ProcessError]",
        nonzero_exit: "Ok(record) with status",
        failure: "Err(ProcessError)",
    },
    RunFormReference {
        form: "run.capture --bytes",
        context: None,
        returns: "Result[{status, stdout: Bytes, stderr: Bytes}, ProcessError]",
        nonzero_exit: "Ok(record) with status",
        failure: "Err(ProcessError)",
    },
    RunFormReference {
        form: "run.stream --text",
        context: None,
        returns: "Result[Stream[Str], ProcessError]",
        nonzero_exit: "Err(ProcessError)",
        failure: "Err(ProcessError)",
    },
    RunFormReference {
        form: "run.stream --bytes",
        context: None,
        returns: "Result[Stream[Bytes], ProcessError]",
        nonzero_exit: "Err(ProcessError)",
        failure: "Err(ProcessError)",
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectReference {
    pub name: &'static str,
    pub covers: &'static [&'static str],
}

pub const EFFECT_REFERENCES: &[EffectReference] = &[
    EffectReference {
        name: "fs",
        covers: &[
            "fs.*",
            "archive.*",
            "diff.*",
            "patch.*",
            "user.*",
            "group.*",
            "module.*",
        ],
    },
    EffectReference {
        name: "io",
        covers: &[
            "io.*",
            "superset of fs",
            "superset of net",
            "superset of process",
            "superset of env",
        ],
    },
    EffectReference {
        name: "net",
        covers: &["net.*", "dns.*"],
    },
    EffectReference {
        name: "process",
        covers: &[
            "run",
            "spawn",
            "wait",
            "ProcessHandle.cancel",
            "effectful process.*",
            "unix.*",
            "linux.*",
            "applet.*",
        ],
    },
    EffectReference {
        name: "env",
        covers: &["env.*", "cd", "system.*"],
    },
    EffectReference {
        name: "time",
        covers: &["time.*", "delayed retry blocks"],
    },
    EffectReference {
        name: "error",
        covers: &["? propagation outside retry attempt blocks"],
    },
];
