use zero_neural::{
    CryptoError, IdentitySigningKey, MachineKeyCapabilities, MachineKeyPair, NeuralKey,
};

/// Derive an identity signing key from a NeuralKey.
///
/// Re-export of `zero_neural::derive_identity_signing_key` for convenience.
pub fn derive_identity_signing_key(
    nk: &NeuralKey,
    identity_id: &[u8; 16],
) -> Result<IdentitySigningKey, CryptoError> {
    zero_neural::derive_identity_signing_key(nk, identity_id)
}

/// Derive a machine key pair from a NeuralKey.
///
/// Re-export of `zero_neural::derive_machine_keypair` for convenience.
pub fn derive_machine_keypair(
    nk: &NeuralKey,
    identity_id: &[u8; 16],
    machine_id: &[u8; 16],
    epoch: u64,
    capabilities: MachineKeyCapabilities,
) -> Result<MachineKeyPair, CryptoError> {
    zero_neural::derive_machine_keypair(nk, identity_id, machine_id, epoch, capabilities)
}
