#![forbid(unsafe_code)]

mod encrypt;
mod error;
mod sector_key;
mod wrap;

pub use encrypt::{decrypt_sector, encrypt_sector};
pub use error::CryptoError;
pub use sector_key::SectorKey;
pub use wrap::{unwrap_sector_key, wrap_sector_key};

#[cfg(test)]
mod tests;
