use super::*;

// The omitted join separator remains absent in the packet. Only the original
// selected default slot authorizes the backend's empty separator behavior.
pub(super) fn encoded_list_join_arguments(store: &FullStore, instruction: u32, count: usize) -> Result<(RuntimeOp, Vec<Option<u32>>, u32), IrVerifyError> {
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprMethod) || count != 2 {
        return Err(IrVerifyError::new("list join changes its original method protocol"));
    }
    let words = store.payload(store.data[instruction as usize].range())?;
    if words.len() != 4 || store.string(words[1])? != "join" {
        return Err(IrVerifyError::new("list join changes its original method spelling"));
    }
    let block = IrBlockId::from_raw(words[2]).and_then(|id| store.blocks.get(id.index()))
        .ok_or_else(|| IrVerifyError::new("list join argument block is invalid"))?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("list join arguments have another block kind")); }
    let mut cursor = FullCursor::new(store.payload(block.instructions)?);
    let supplied = cursor.raw()?;
    if supplied > 1 { return Err(IrVerifyError::new("list join changes its original supplied argument count")); }
    let separator = if supplied == 0 { None } else { Some(cursor.raw()?) };
    cursor.finish()?;
    Ok((RuntimeOp::TextJoin, vec![Some(words[0]), separator], words[3]))
}
