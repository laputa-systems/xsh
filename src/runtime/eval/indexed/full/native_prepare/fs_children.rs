use super::*;

pub(super) fn encoded_fs_children_arguments(store: &FullStore, instruction: u32, count: usize) -> Result<(RuntimeOp, Vec<Option<u32>>, u32), IrVerifyError> {
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprFsList) || count != 3 {
        return Err(IrVerifyError::new("filesystem children changes its original operand protocol"));
    }
    let mut cursor = FullCursor::new(store.payload(store.data[instruction as usize].range())?);
    let operation = *store.runtime_ops.get(cursor.raw()? as usize).ok_or_else(|| IrVerifyError::new("filesystem children operation is invalid"))?;
    if operation != RuntimeOp::FsChildren { return Err(IrVerifyError::new("filesystem children changes its original selected operation")); }
    let path = cursor.raw()?;
    let mut optional = || match cursor.raw()? {
        0 => Ok(None), 1 => Ok(Some(cursor.raw()?)),
        _ => Err(IrVerifyError::new("filesystem children option marker is invalid")),
    };
    let stat = optional()?;
    let ordered = optional()?;
    let location = cursor.raw()?;
    cursor.finish()?;
    Ok((operation, vec![Some(path), stat, ordered], location))
}
