use crate::types::Type;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorField {
    pub name: &'static str,
    pub ty: Type,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ErrorVariant {
    pub name: &'static str,
    pub facets: &'static [&'static str],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorFamily {
    pub name: &'static str,
    pub fields: Vec<ErrorField>,
    pub variants: &'static [ErrorVariant],
}

pub fn builtin_error_families() -> Vec<ErrorFamily> {
    vec![process_error_family()]
}

pub fn process_error_family() -> ErrorFamily {
    ErrorFamily {
        name: "ProcessError",
        fields: vec![
            ErrorField {
                name: "message",
                ty: Type::Str,
            },
            ErrorField {
                name: "status",
                ty: Type::Optional(Box::new(Type::Status)),
            },
        ],
        variants: PROCESS_ERROR_VARIANTS,
    }
}

pub const PROCESS_ERROR_VARIANTS: &[ErrorVariant] = &[
    ErrorVariant {
        name: "NotFound",
        facets: &["NotFound"],
    },
    ErrorVariant {
        name: "PermissionDenied",
        facets: &["PermissionDenied"],
    },
    ErrorVariant {
        name: "NonzeroExit",
        facets: &["NonzeroExit"],
    },
    ErrorVariant {
        name: "Signal",
        facets: &["Signal"],
    },
    ErrorVariant {
        name: "Timeout",
        facets: &["Timeout"],
    },
    ErrorVariant {
        name: "Canceled",
        facets: &["Canceled"],
    },
    ErrorVariant {
        name: "CaptureLimit",
        facets: &["CaptureLimit"],
    },
    ErrorVariant {
        name: "InvalidUtf8",
        facets: &["InvalidData"],
    },
    ErrorVariant {
        name: "PipelineFailure",
        facets: &["ProcessFailure"],
    },
    ErrorVariant {
        name: "ExecFailure",
        facets: &["ProcessFailure"],
    },
    ErrorVariant {
        name: "Spawn",
        facets: &["ProcessFailure"],
    },
    ErrorVariant {
        name: "Io",
        facets: &["HostIo"],
    },
    ErrorVariant {
        name: "Redirection",
        facets: &["HostIo"],
    },
    ErrorVariant {
        name: "InvalidTarget",
        facets: &["InvalidData"],
    },
    ErrorVariant {
        name: "Unknown",
        facets: &[],
    },
];
