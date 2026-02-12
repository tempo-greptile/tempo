//! Signature Verification Precompile (TIP-1020)
//!
//! Enables contracts to verify Tempo signature types (secp256k1, P256, WebAuthn)
//! using the same verification logic as Tempo transaction processing.

pub mod dispatch;

use crate::{SIGNATURE_VERIFICATION_ADDRESS, error::Result};
use alloy::primitives::{Address, B256};
use tempo_contracts::precompiles::{
    ISignatureVerification::verifyCall, SignatureVerificationError,
};
use tempo_precompiles_macros::contract;
use tempo_primitives::transaction::{
    precompile_signature_verification_gas,
    tt_signature::TempoSignature,
};

pub use tempo_contracts::precompiles::ISignatureVerification;

/// Signature Verification precompile
#[contract(addr = SIGNATURE_VERIFICATION_ADDRESS)]
pub struct SignatureVerification {}

impl SignatureVerification {
    /// Verify a Tempo signature
    ///
    /// Returns true if the signature is valid and the recovered signer matches.
    /// Reverts with appropriate error otherwise.
    pub fn verify(&mut self, call: verifyCall) -> Result<bool> {
        let signer = call.signer;
        let hash = call.hash;
        let signature_bytes = call.signature;

        let signature = TempoSignature::from_bytes(&signature_bytes)
            .map_err(|_| SignatureVerificationError::invalid_signature())?;

        // Keychain signatures require stateful authorization checks, so they are rejected here.
        if signature.is_keychain() {
            return Err(SignatureVerificationError::invalid_signature().into());
        }

        let verification_gas = precompile_signature_verification_gas(&signature);
        self.storage.deduct_gas(verification_gas)?;

        let recovered = signature
            .recover_signer(&hash)
            .map_err(|_| SignatureVerificationError::invalid_signature())?;

        if recovered != signer {
            return Err(SignatureVerificationError::signer_mismatch(signer, recovered).into());
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{StorageCtx, hashmap::HashMapStorageProvider};
    use alloy::primitives::{Address, Bytes, keccak256};
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;
    use tempo_primitives::transaction::PrimitiveSignature;

    /// Helper to create a secp256k1 signature for a hash
    fn sign_secp256k1(signer: &PrivateKeySigner, hash: &B256) -> TempoSignature {
        let sig = signer.sign_hash_sync(hash).unwrap();
        TempoSignature::Primitive(PrimitiveSignature::Secp256k1(sig))
    }

    #[test]
    fn test_verify_secp256k1_valid() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        StorageCtx::enter(&mut storage, || {
            let mut precompile = SignatureVerification::new();

            let signer = PrivateKeySigner::random();
            let signer_addr = signer.address();
            let message_hash = keccak256(b"test message");

            let tempo_sig = sign_secp256k1(&signer, &message_hash);
            let sig_bytes = Bytes::from(tempo_sig.to_bytes());

            let call = verifyCall {
                signer: signer_addr,
                hash: message_hash,
                signature: sig_bytes,
            };

            let result = precompile.verify(call)?;
            assert!(result, "Valid secp256k1 signature should return true");

            Ok(())
        })
    }

    #[test]
    fn test_verify_secp256k1_wrong_signer() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        StorageCtx::enter(&mut storage, || {
            let mut precompile = SignatureVerification::new();

            let actual_signer = PrivateKeySigner::random();
            let wrong_signer = Address::random();
            let message_hash = keccak256(b"test message");

            let tempo_sig = sign_secp256k1(&actual_signer, &message_hash);
            let sig_bytes = Bytes::from(tempo_sig.to_bytes());

            let call = verifyCall {
                signer: wrong_signer,
                hash: message_hash,
                signature: sig_bytes,
            };

            let result = precompile.verify(call);
            assert!(result.is_err(), "Wrong signer should fail");

            Ok(())
        })
    }

    #[test]
    fn test_verify_secp256k1_wrong_hash() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        StorageCtx::enter(&mut storage, || {
            let mut precompile = SignatureVerification::new();

            let signer = PrivateKeySigner::random();
            let signer_addr = signer.address();
            let signed_hash = keccak256(b"original message");
            let wrong_hash = keccak256(b"different message");

            let tempo_sig = sign_secp256k1(&signer, &signed_hash);
            let sig_bytes = Bytes::from(tempo_sig.to_bytes());

            let call = verifyCall {
                signer: signer_addr,
                hash: wrong_hash,
                signature: sig_bytes,
            };

            let result = precompile.verify(call);
            assert!(result.is_err(), "Wrong hash should fail (signer mismatch)");

            Ok(())
        })
    }

    #[test]
    fn test_verify_invalid_signature_bytes() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        StorageCtx::enter(&mut storage, || {
            let mut precompile = SignatureVerification::new();

            let signer_addr = Address::random();
            let message_hash = keccak256(b"test message");

            // Completely invalid signature bytes (wrong length)
            let invalid_sig = Bytes::from(vec![0u8; 10]);

            let call = verifyCall {
                signer: signer_addr,
                hash: message_hash,
                signature: invalid_sig,
            };

            let result = precompile.verify(call);
            assert!(result.is_err(), "Invalid signature bytes should fail");

            Ok(())
        })
    }

    #[test]
    fn test_verify_empty_signature() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        StorageCtx::enter(&mut storage, || {
            let mut precompile = SignatureVerification::new();

            let signer_addr = Address::random();
            let message_hash = keccak256(b"test message");

            let call = verifyCall {
                signer: signer_addr,
                hash: message_hash,
                signature: Bytes::new(),
            };

            let result = precompile.verify(call);
            assert!(result.is_err(), "Empty signature should fail");

            Ok(())
        })
    }

    #[test]
    fn test_gas_calculation_secp256k1() {
        let signer = PrivateKeySigner::random();
        let hash = keccak256(b"test");
        let sig = sign_secp256k1(&signer, &hash);

        let gas = precompile_signature_verification_gas(&sig);
        // ECRECOVER_GAS (3000) + tempo_signature_verification_gas for secp256k1 (0)
        assert_eq!(gas, 3000, "secp256k1 should cost 3000 gas");
    }

    #[test]
    fn test_gas_calculation_p256() {
        use tempo_primitives::transaction::tt_signature::P256SignatureWithPreHash;

        let p256_sig = P256SignatureWithPreHash {
            r: B256::ZERO,
            s: B256::ZERO,
            pub_key_x: B256::ZERO,
            pub_key_y: B256::ZERO,
            pre_hash: false,
        };
        let sig = TempoSignature::Primitive(PrimitiveSignature::P256(p256_sig));

        let gas = precompile_signature_verification_gas(&sig);
        // ECRECOVER_GAS (3000) + P256_VERIFY_GAS (5000) = 8000
        assert_eq!(gas, 8000, "P256 should cost 8000 gas");
    }

    #[test]
    fn test_keychain_signature_rejected() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        StorageCtx::enter(&mut storage, || {
            let mut precompile = SignatureVerification::new();

            let root_signer = PrivateKeySigner::random();
            let root_addr = root_signer.address();
            let access_key_signer = PrivateKeySigner::random();
            let message_hash = keccak256(b"test keychain message");

            let access_sig = access_key_signer.sign_hash_sync(&message_hash)?;
            let inner_sig = PrimitiveSignature::Secp256k1(access_sig);
            let keychain_sig =
                TempoSignature::Keychain(tempo_primitives::transaction::KeychainSignature::new(
                    root_addr,
                    inner_sig,
                ));
            let sig_bytes = Bytes::from(keychain_sig.to_bytes());

            let call = verifyCall {
                signer: root_addr,
                hash: message_hash,
                signature: sig_bytes,
            };

            let result = precompile.verify(call);
            assert!(result.is_err(), "Keychain signatures should be rejected");

            Ok(())
        })
    }
}
