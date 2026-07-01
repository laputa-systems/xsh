use super::session::Session;

pub(super) fn prompt(session: &Session) -> String {
    let user = session.user.as_deref().unwrap_or("xshi");
    let host = session.host.as_deref().unwrap_or("host");
    let cwd = shorten_path(session);
    let git = session
        .git_prompt
        .as_deref()
        .map(|name| format!(" ({name})"))
        .unwrap_or_default();
    let denv = if session.denv.dirty { " *" } else { "" };
    let marker = if session.last_status == 0 { "$" } else { "!" };
    if session.colors {
        let color = if session.last_status == 0 { "32" } else { "31" };
        format!(
            "\x1b[2m{user}@{host}\x1b[0m \x1b[34m{cwd}\x1b[0m\x1b[33m{git}{denv}\x1b[0m \x1b[{color}m{marker}\x1b[0m "
        )
    } else {
        format!("{user}@{host} {cwd}{git}{denv} {marker} ")
    }
}

#[allow(clippy::single_call_fn)]
fn shorten_path(session: &Session) -> String {
    if let Some(home) = &session.home
        && let Ok(stripped) = session.cwd.strip_prefix(home)
    {
        if stripped.as_os_str().is_empty() {
            return "~".to_string();
        }
        return format!("~/{}", stripped.display());
    }
    session.cwd.display().to_string()
}
