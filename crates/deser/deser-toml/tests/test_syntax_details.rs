use bbtree::unsync::BBTreeMap as BTreeMap;
use deser::ErrorKind;
use deser_toml::{SyntaxDetail, from_str};

#[test]
fn duplicate_syntax_retains_its_typed_detail_and_location() {
    for input in ["a = 1\na = 2\n", "[a]\nx = 1\n[a]\ny = 2\n", "a = {b = 1, b = 2}\n"] {
        let error = from_str::<BTreeMap<String, String>>(input).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Syntax);
        assert_eq!(error.attachment::<SyntaxDetail>(), Some(&SyntaxDetail::DuplicateKey));
        assert!(error.offset().is_some());
        assert!(error.line().is_some());
    }
}

#[test]
fn numeric_overflow_retains_its_typed_detail_without_reclassifying_syntax() {
    for input in ["a = 18446744073709551616", "a = -9223372036854775809", "a = 0x1_0000_0000_0000_0000", "a = 1e400"] {
        let error = from_str::<BTreeMap<String, String>>(input).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Syntax);
        assert_eq!(error.attachment::<SyntaxDetail>(), Some(&SyntaxDetail::NumberOverflow));
        assert!(error.column().is_some());
    }
    let error = from_str::<BTreeMap<String, String>>("a = 01").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Syntax);
    assert!(error.attachment::<SyntaxDetail>().is_none());
}
