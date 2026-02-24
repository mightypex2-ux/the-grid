#![forbid(unsafe_code)]

pub mod derivation;
pub mod did;
pub mod encapsulation;
pub mod error;
pub mod machine_key;
pub mod neural_key;
pub mod signing;

pub use derivation::{derive_identity_signing_key, derive_machine_keypair};
pub use did::{did_key_to_ed25519, ed25519_to_did_key};
pub use encapsulation::{EncapBundle, SharedSecret};
pub use error::CryptoError;
pub use machine_key::{MachineKeyCapabilities, MachineKeyPair, MachinePublicKey};
pub use neural_key::NeuralKey;
pub use signing::{HybridSignature, IdentitySigningKey, IdentityVerifyingKey};
