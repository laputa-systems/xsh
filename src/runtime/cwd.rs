use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CwdContext {
    pub path: PathBuf,
}
