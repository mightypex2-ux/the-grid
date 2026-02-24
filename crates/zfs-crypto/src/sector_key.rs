use rand_core::OsRng;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// 256-bit symmetric key for sector encryption (XChaCha20-Poly1305).
///
/// Generated via CSPRNG. Automatically zeroized on drop to prevent
/// key material from lingering in memory.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SectorKey([u8; 32]);

impl SectorKey {
    /// Generate a fresh random SectorKey using the OS CSPRNG.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand_core::RngCore::fill_bytes(&mut OsRng, &mut bytes);
        Self(bytes)
    }

    /// Create a SectorKey from raw bytes. Caller is responsible for
    /// ensuring the bytes came from a secure source.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Access the raw 32-byte key material.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for SectorKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SectorKey([REDACTED])")
    }
}
