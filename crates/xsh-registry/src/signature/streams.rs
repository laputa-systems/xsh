use super::{Type, fs_entry_type};
pub(in crate::signature) fn fs_entry_stream() -> Type {
    Type::Stream(Box::new(fs_entry_type()))
}
