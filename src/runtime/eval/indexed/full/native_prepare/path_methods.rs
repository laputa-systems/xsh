use super::*;

pub(super) fn path_method_operation_is_supported(operation: RuntimeOp) -> bool {
    matches!(operation, RuntimeOp::PathDisplay | RuntimeOp::PathName | RuntimeOp::PathParent | RuntimeOp::PathExt | RuntimeOp::PathNormalize | RuntimeOp::PathWithExt | RuntimeOp::PathStripPrefix | RuntimeOp::PathRelativeTo)
}

pub(super) fn encoded_path_method_arguments(store: &FullStore, instruction: u32, count: usize, name: Name, operation: RuntimeOp) -> Result<(RuntimeOp, Vec<Option<u32>>, u32), IrVerifyError> {
    match store.tags.get(instruction as usize) {
        Some(FullTag::ExprPathReadText | FullTag::ExprPathReadBytes) => {
            let (selected, spelling) = if store.tags[instruction as usize] == FullTag::ExprPathReadText { (RuntimeOp::FsReadText, "read_text") } else { (RuntimeOp::FsRead, "read_bytes") };
            let words = store.payload(store.data[instruction as usize].range())?;
            if words.len() != 2 || count != 1 || operation != selected || name != Name::intern(spelling) {
                return Err(IrVerifyError::new("Path read changes its original selected operation, spelling or hidden receiver packet"));
            }
            Ok((selected, vec![Some(words[0])], words[1]))
        }
        Some(FullTag::ExprModuleCall) => {
            let encoded = encoded_native_arguments(store, instruction, count)?;
            if encoded.0 != operation || encoded.1.first().is_none_or(Option::is_none) {
                return Err(IrVerifyError::new("Path method changes its selected module packet or hidden receiver"));
            }
            Ok(encoded)
        }
        Some(FullTag::ExprMethod) => {
            if !path_method_operation_is_supported(operation) { return Err(IrVerifyError::new("Path method selected operation has no prepared method execution protocol")); }
            let (arguments, location) = encoded_native_method_arguments(store, instruction, count, name)?;
            Ok((operation, arguments, location))
        }
        _ => Err(IrVerifyError::new("Path native method proof is attached to another opcode")),
    }
}
