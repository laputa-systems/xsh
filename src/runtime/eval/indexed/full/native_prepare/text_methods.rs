use super::*;

// A trailing omitted text limit remains absent in the method packet. The
// selected binding certifies that omission; no numeric default is synthesized.
pub(super) fn encoded_text_method_arguments(store: &FullStore, instruction: u32, count: usize, operation: RuntimeOp) -> Result<(RuntimeOp, Vec<Option<u32>>, u32), IrVerifyError> {
    let spelling = match operation { RuntimeOp::TextSplit => "split", RuntimeOp::TextByteSlice => "byte_slice", _ => return Err(IrVerifyError::new("text method has another selected operation")) };
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprMethod) || count != 3 {
        return Err(IrVerifyError::new("text method changes its original method protocol"));
    }
    let words = store.payload(store.data[instruction as usize].range())?;
    if words.len() != 4 || store.string(words[1])? != spelling {
        return Err(IrVerifyError::new("text method changes its original method spelling"));
    }
    let block = IrBlockId::from_raw(words[2]).and_then(|id| store.blocks.get(id.index()))
        .ok_or_else(|| IrVerifyError::new("text method argument block is invalid"))?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("text method arguments have another block kind")); }
    let mut cursor = FullCursor::new(store.payload(block.instructions)?);
    let count = cursor.raw()?;
    if !matches!(count, 1 | 2) { return Err(IrVerifyError::new("text method changes its original supplied argument count")); }
    let mut arguments = vec![Some(words[0]), Some(cursor.raw()?), None];
    if count == 2 { arguments[2] = Some(cursor.raw()?); }
    cursor.finish()?;
    Ok((operation, arguments, words[3]))
}
