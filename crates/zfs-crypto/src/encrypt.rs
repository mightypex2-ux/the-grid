use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
use chacha20poly1305::{AeadCore, XChaCha20Poly1305, XNonce};

use crate::{CryptoError, SectorKey};

const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;

/// Encrypt a sector payload with XChaCha20-Poly1305.
///
/// Returns `nonce (24) || ciphertext || tag (16)`.
/// AAD should be `program_id || sector_id` to bind ciphertext to its context.
pub fn encrypt_sector(
    plaintext: &[u8],
    key: &SectorKey,
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);

    let payload = chacha20poly1305::aead::Payload {
        msg: plaintext,
        aad,
    };
    let ciphertext = cipher
        .encrypt(&nonce, payload)
        .map_err(|_| CryptoError::EncryptionFailed)?;

    let mut sealed = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    sealed.extend_from_slice(&nonce);
    sealed.extend_from_slice(&ciphertext);
    Ok(sealed)
}

/// Decrypt a sector payload encrypted with [`encrypt_sector`].
///
/// Expects `sealed = nonce (24) || ciphertext || tag (16)`.
/// AAD must match the value used during encryption.
pub fn decrypt_sector(sealed: &[u8], key: &SectorKey, aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let min_len = NONCE_LEN + TAG_LEN;
    if sealed.len() < min_len {
        return Err(CryptoError::CiphertextTooShort {
            len: sealed.len(),
            min: min_len,
        });
    }

    let (nonce_bytes, ct) = sealed.split_at(NONCE_LEN);
    let nonce = XNonce::from_slice(nonce_bytes);

    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());
    let payload = chacha20poly1305::aead::Payload { msg: ct, aad };

    cipher
        .decrypt(nonce, payload)
        .map_err(|_| CryptoError::DecryptionFailed)
}
