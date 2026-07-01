use crate::source::Span;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SignalHookRejection {
    Numeric,
    Unknown,
    Uncatchable,
    EventLike,
    Pipe,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HookSignal {
    pub(crate) name: String,
    pub(crate) number: i32,
}

pub(crate) fn normalize_hook_signal(
    signal: &str,
    span: Span,
) -> Result<HookSignal, SignalHookRejection> {
    if signal.parse::<i32>().is_ok() {
        return Err(SignalHookRejection::Numeric);
    }
    let upper = signal.to_ascii_uppercase();
    let name = upper.strip_prefix("SIG").unwrap_or(&upper);
    match name {
        "KILL" | "STOP" => return Err(SignalHookRejection::Uncatchable),
        "CHLD" | "CONT" | "TSTP" | "TTIN" | "TTOU" => {
            return Err(SignalHookRejection::EventLike);
        }
        "PIPE" => return Err(SignalHookRejection::Pipe),
        "HUP" | "INT" | "QUIT" | "TERM" | "USR1" | "USR2" | "ALRM" | "XCPU" | "XFSZ" => {}
        _ => return Err(SignalHookRejection::Unknown),
    }
    signal_number(name, span)
        .map(|number| HookSignal {
            name: name.to_string(),
            number,
        })
        .ok_or(SignalHookRejection::Unsupported)
}

pub(crate) fn hook_signal_from_number(number: i32) -> HookSignal {
    HookSignal {
        name: signal_name(number)
            .map(str::to_string)
            .unwrap_or_else(|| number.to_string()),
        number,
    }
}

fn signal_number(name: &str, _span: Span) -> Option<i32> {
    Some(match name {
        "HUP" => libc::SIGHUP,
        "INT" => libc::SIGINT,
        "QUIT" => libc::SIGQUIT,
        "TERM" => libc::SIGTERM,
        "USR1" => libc::SIGUSR1,
        "USR2" => libc::SIGUSR2,
        "ALRM" => libc::SIGALRM,
        "XCPU" => libc::SIGXCPU,
        "XFSZ" => libc::SIGXFSZ,
        _ => return None,
    })
}

fn signal_name(number: i32) -> Option<&'static str> {
    [
        ("HUP", libc::SIGHUP),
        ("INT", libc::SIGINT),
        ("QUIT", libc::SIGQUIT),
        ("TERM", libc::SIGTERM),
        ("USR1", libc::SIGUSR1),
        ("USR2", libc::SIGUSR2),
        ("ALRM", libc::SIGALRM),
        ("XCPU", libc::SIGXCPU),
        ("XFSZ", libc::SIGXFSZ),
    ]
    .iter()
    .find_map(|(name, candidate)| (*candidate == number).then_some(*name))
}

pub(crate) fn signal_rejection_message(signal: &str, rejection: SignalHookRejection) -> String {
    match rejection {
        SignalHookRejection::Numeric => "signal hooks require a named signal".to_string(),
        SignalHookRejection::Unknown => format!("unknown signal hook `{signal}`"),
        SignalHookRejection::Uncatchable => {
            format!("signal `{signal}` cannot be caught by a signal hook")
        }
        SignalHookRejection::EventLike => {
            format!("signal `{signal}` is outside the v1 shutdown hook surface")
        }
        SignalHookRejection::Pipe => "`PIPE` hooks are not supported in v1".to_string(),
        SignalHookRejection::Unsupported => {
            format!("signal `{signal}` is not available on this platform")
        }
    }
}
