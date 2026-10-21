use crate::runtime::value::RuntimeError;
use crate::source::Span;
use graviola::Error as GraviolaError;
use graviola::signing::eddsa::Ed25519VerifyingKey;

pub(crate) fn ed25519_verify(
    message: &[u8],
    public_key: &[u8],
    signature: &[u8],
    span: Span,
) -> Result<bool, RuntimeError> {
    if public_key.len() != 32 {
        return Err(RuntimeError::new(
            "crypto-ed25519-public-key-length",
            "Ed25519 public key must be exactly 32 bytes",
        )
        .with_span(span));
    }
    if signature.len() != 64 {
        return Err(RuntimeError::new(
            "crypto-ed25519-signature-length",
            "Ed25519 signature must be exactly 64 bytes",
        )
        .with_span(span));
    }

    let key = Ed25519VerifyingKey::from_bytes(public_key).map_err(|error| match error {
        GraviolaError::NotOnCurve => RuntimeError::new(
            "crypto-ed25519-public-key",
            format!("invalid Ed25519 public key: {error}"),
        )
        .with_span(span),
        _ => RuntimeError::new(
            "crypto-ed25519-provider",
            format!("Ed25519 verification provider failed: {error}"),
        )
        .with_span(span),
    })?;

    match key.verify(signature, message) {
        Ok(()) => Ok(true),
        Err(GraviolaError::BadSignature) => Ok(false),
        Err(error) => Err(RuntimeError::new(
            "crypto-ed25519-provider",
            format!("Ed25519 verification provider failed: {error}"),
        )
        .with_span(span)),
    }
}
