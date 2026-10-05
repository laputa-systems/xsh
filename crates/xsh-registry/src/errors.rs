use crate::types::Type;

/// The built-in error facet vocabulary: the cross-family categories that
/// `is Facet` and `Err(is Facet)` patterns match. Built-in families and host
/// operations implement only these facets. User `error` declarations may name
/// these or declare their own, and facets always compare by name, so a user
/// variant declared `: NotFound` matches `is NotFound` like a missing file does.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum ErrorFacet {
    NotFound,
    PermissionDenied,
    NonzeroExit,
    Signal,
    Timeout,
    Canceled,
    CaptureLimit,
    InvalidData,
    HostIo,
    ProcessFailure,
    MissingExport,
    MismatchedExport,
    UnexpectedExport,
}

impl ErrorFacet {
    pub const ALL: &'static [Self] = &[
        Self::NotFound,
        Self::PermissionDenied,
        Self::NonzeroExit,
        Self::Signal,
        Self::Timeout,
        Self::Canceled,
        Self::CaptureLimit,
        Self::InvalidData,
        Self::HostIo,
        Self::ProcessFailure,
        Self::MissingExport,
        Self::MismatchedExport,
        Self::UnexpectedExport,
    ];

    /// The source spelling in `is Facet` patterns and `error` declarations.
    pub const fn name(self) -> &'static str {
        match self {
            Self::NotFound => "NotFound",
            Self::PermissionDenied => "PermissionDenied",
            Self::NonzeroExit => "NonzeroExit",
            Self::Signal => "Signal",
            Self::Timeout => "Timeout",
            Self::Canceled => "Canceled",
            Self::CaptureLimit => "CaptureLimit",
            Self::InvalidData => "InvalidData",
            Self::HostIo => "HostIo",
            Self::ProcessFailure => "ProcessFailure",
            Self::MissingExport => "MissingExport",
            Self::MismatchedExport => "MismatchedExport",
            Self::UnexpectedExport => "UnexpectedExport",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|facet| facet.name() == name)
    }

    /// What a failure implementing the facet means.
    pub const fn summary(self) -> &'static str {
        match self {
            Self::NotFound => "The target file, path, or command does not exist.",
            Self::PermissionDenied => "The host refused access to the target.",
            Self::NonzeroExit => "A process exited with a nonzero status.",
            Self::Signal => "A process was terminated by a signal.",
            Self::Timeout => "The operation exceeded its time limit.",
            Self::Canceled => "Process work was canceled before it completed.",
            Self::CaptureLimit => "Captured process output exceeded its limit.",
            Self::InvalidData => {
                "Data was malformed, such as invalid UTF-8 or a NUL byte in a target."
            }
            Self::HostIo => "Any other host I/O failure.",
            Self::ProcessFailure => "A process could not be spawned, executed, or completed.",
            Self::MissingExport => "A module lacks an export its contract requires.",
            Self::MismatchedExport => {
                "A module export has another kind or signature than its contract declares."
            }
            Self::UnexpectedExport => "A module has an export its exact contract does not list.",
        }
    }

    /// The module-contract check that raises the facet, for facets no
    /// built-in family variant or host OS error implements.
    pub const fn contract_check_source(self) -> Option<&'static str> {
        match self {
            Self::MissingExport => {
                Some("a failed `.require(Contract)` on a module without a required export")
            }
            Self::MismatchedExport => Some(
                "a failed `.require(Contract)` on a module whose export differs from the contract",
            ),
            Self::UnexpectedExport => Some(
                "a failed `.require(Contract)` on a module with an export outside an `exact module` contract",
            ),
            _ => None,
        }
    }

    /// The facet a host operation's OS error implements. Filesystem, path, and
    /// OS calls outside process forms carry exactly this one facet.
    pub fn of_host_io(kind: std::io::ErrorKind) -> Self {
        HOST_IO_FACETS
            .iter()
            .find(|(host_kind, _)| *host_kind == kind)
            .map_or(Self::HostIo, |(_, facet)| *facet)
    }

    /// The OS error kinds `of_host_io` maps onto this facet by name. `HostIo`
    /// lists none here because it receives every unlisted kind.
    pub fn host_io_kinds(self) -> impl Iterator<Item = std::io::ErrorKind> {
        HOST_IO_FACETS
            .iter()
            .filter(move |(_, facet)| *facet == self)
            .map(|(kind, _)| *kind)
    }
}

const HOST_IO_FACETS: &[(std::io::ErrorKind, ErrorFacet)] = &[
    (std::io::ErrorKind::NotFound, ErrorFacet::NotFound),
    (
        std::io::ErrorKind::PermissionDenied,
        ErrorFacet::PermissionDenied,
    ),
    (std::io::ErrorKind::TimedOut, ErrorFacet::Timeout),
    (std::io::ErrorKind::InvalidData, ErrorFacet::InvalidData),
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorField {
    pub name: &'static str,
    pub ty: Type,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ErrorVariant {
    pub name: &'static str,
    pub facets: &'static [ErrorFacet],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorFamily {
    pub name: &'static str,
    pub fields: Vec<ErrorField>,
    pub variants: &'static [ErrorVariant],
}

pub fn builtin_error_families() -> Vec<ErrorFamily> {
    vec![process_error_family(), assertion_error_family()]
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

/// The facets a `ProcessError` variant implements. Fails loudly on a name
/// outside the family because runtime variant names come from a closed table.
pub fn process_error_facets(variant: &str) -> &'static [ErrorFacet] {
    PROCESS_ERROR_VARIANTS
        .iter()
        .find(|candidate| candidate.name == variant)
        .unwrap_or_else(|| panic!("`{variant}` is not a ProcessError variant"))
        .facets
}

pub const PROCESS_ERROR_VARIANTS: &[ErrorVariant] = &[
    ErrorVariant {
        name: "NotFound",
        facets: &[ErrorFacet::NotFound],
    },
    ErrorVariant {
        name: "PermissionDenied",
        facets: &[ErrorFacet::PermissionDenied],
    },
    ErrorVariant {
        name: "NonzeroExit",
        facets: &[ErrorFacet::NonzeroExit],
    },
    ErrorVariant {
        name: "UnexpectedExit",
        facets: &[ErrorFacet::ProcessFailure],
    },
    ErrorVariant {
        name: "Signal",
        facets: &[ErrorFacet::Signal],
    },
    ErrorVariant {
        name: "Timeout",
        facets: &[ErrorFacet::Timeout],
    },
    ErrorVariant {
        name: "Canceled",
        facets: &[ErrorFacet::Canceled],
    },
    ErrorVariant {
        name: "CaptureLimit",
        facets: &[ErrorFacet::CaptureLimit],
    },
    ErrorVariant {
        name: "InvalidUtf8",
        facets: &[ErrorFacet::InvalidData],
    },
    ErrorVariant {
        name: "PipelineFailure",
        facets: &[ErrorFacet::ProcessFailure],
    },
    ErrorVariant {
        name: "ExecFailure",
        facets: &[ErrorFacet::ProcessFailure],
    },
    ErrorVariant {
        name: "Spawn",
        facets: &[ErrorFacet::ProcessFailure],
    },
    ErrorVariant {
        name: "Io",
        facets: &[ErrorFacet::HostIo],
    },
    ErrorVariant {
        name: "Redirection",
        facets: &[ErrorFacet::HostIo],
    },
    ErrorVariant {
        name: "InvalidTarget",
        facets: &[ErrorFacet::InvalidData],
    },
    ErrorVariant {
        name: "Unknown",
        facets: &[],
    },
];

pub fn assertion_error_family() -> ErrorFamily {
    ErrorFamily {
        name: "AssertionError",
        fields: vec![ErrorField {
            name: "message",
            ty: Type::Str,
        }],
        variants: &[ErrorVariant {
            name: "Failed",
            facets: &[],
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facet_names_round_trip() {
        for facet in ErrorFacet::ALL {
            assert_eq!(ErrorFacet::from_name(facet.name()), Some(*facet));
        }
        assert_eq!(ErrorFacet::from_name("Missing"), None);
    }

    #[test]
    fn every_facet_is_implemented_by_a_builtin_source() {
        for facet in ErrorFacet::ALL {
            let by_variant = builtin_error_families().iter().any(|family| {
                family
                    .variants
                    .iter()
                    .any(|variant| variant.facets.contains(facet))
            });
            let by_host = *facet == ErrorFacet::HostIo || facet.host_io_kinds().next().is_some();
            assert!(
                by_variant || by_host,
                "{} has no built-in source",
                facet.name()
            );
        }
    }

    #[test]
    fn host_io_errors_map_onto_one_facet() {
        assert_eq!(
            ErrorFacet::of_host_io(std::io::ErrorKind::NotFound),
            ErrorFacet::NotFound
        );
        assert_eq!(
            ErrorFacet::of_host_io(std::io::ErrorKind::TimedOut),
            ErrorFacet::Timeout
        );
        assert_eq!(
            ErrorFacet::of_host_io(std::io::ErrorKind::AlreadyExists),
            ErrorFacet::HostIo
        );
    }
}
