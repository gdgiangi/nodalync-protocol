//! Validator trait and default implementation.
//!
//! This module provides the main `Validator` trait that combines all
//! validation functions, as well as a default implementation.

use nodalync_crypto::{PublicKey, Timestamp};
use nodalync_types::{Channel, Manifest, Payment, PeerId};
use nodalync_wire::Message;

use crate::access::validate_access_with_owner_bypass;
use crate::content::validate_content;
use crate::error::ValidationResult;
use crate::message::validate_message;
use crate::payment::{validate_payment, BondChecker, PublicKeyLookup};
use crate::provenance::validate_provenance;
use crate::version::validate_version;

/// Trait for validating protocol entities.
///
/// This trait combines all validation rules from Protocol Specification §9.
/// Implementations can customize validation behavior, such as providing
/// public key lookup or bond checking functionality.
pub trait Validator {
    /// Validate content against its manifest.
    ///
    /// See §9.1 for validation rules.
    fn validate_content(&self, content: &[u8], manifest: &Manifest) -> ValidationResult<()>;

    /// Validate version constraints.
    ///
    /// See §9.2 for validation rules.
    fn validate_version(
        &self,
        manifest: &Manifest,
        previous: Option<&Manifest>,
    ) -> ValidationResult<()>;

    /// Validate provenance chain.
    ///
    /// See §9.3 for validation rules.
    fn validate_provenance(
        &self,
        manifest: &Manifest,
        sources: &[Manifest],
    ) -> ValidationResult<()>;

    /// Validate a payment.
    ///
    /// See §9.4 for validation rules.
    fn validate_payment(
        &self,
        payment: &Payment,
        channel: &Channel,
        manifest: &Manifest,
    ) -> ValidationResult<()>;

    /// Validate a protocol message.
    ///
    /// See §9.5 for validation rules.
    fn validate_message(&self, message: &Message) -> ValidationResult<()>;

    /// Validate access permissions.
    ///
    /// See §9.6 for validation rules.
    fn validate_access(&self, requester: &PeerId, manifest: &Manifest) -> ValidationResult<()>;
}

/// Configuration for the default validator.
#[derive(Clone, Default)]
pub struct ValidatorConfig {
    /// Current timestamp provider
    current_time: Option<Timestamp>,
}

impl ValidatorConfig {
    /// Create a new validator config.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a fixed timestamp for testing.
    pub fn with_fixed_time(mut self, timestamp: Timestamp) -> Self {
        self.current_time = Some(timestamp);
        self
    }
}

/// Default validator implementation.
///
/// This validator uses the standalone validation functions from each module.
/// It can be customized with callbacks for public key lookup and bond checking.
pub struct DefaultValidator<P = NoopPublicKeyLookup, B = NoopBondChecker>
where
    P: PublicKeyLookup,
    B: BondChecker,
{
    config: ValidatorConfig,
    pubkey_lookup: P,
    bond_checker: B,
}

impl DefaultValidator<NoopPublicKeyLookup, NoopBondChecker> {
    /// Create a new default validator with no external dependencies.
    ///
    /// This validator will skip signature verification and bond checking.
    pub fn new() -> Self {
        Self {
            config: ValidatorConfig::default(),
            pubkey_lookup: NoopPublicKeyLookup,
            bond_checker: NoopBondChecker,
        }
    }

    /// Create a new default validator with configuration.
    pub fn with_config(config: ValidatorConfig) -> Self {
        Self {
            config,
            pubkey_lookup: NoopPublicKeyLookup,
            bond_checker: NoopBondChecker,
        }
    }
}

impl<P, B> DefaultValidator<P, B>
where
    P: PublicKeyLookup,
    B: BondChecker,
{
    /// Create a validator with custom public key lookup and bond checker.
    pub fn with_dependencies(config: ValidatorConfig, pubkey_lookup: P, bond_checker: B) -> Self {
        Self {
            config,
            pubkey_lookup,
            bond_checker,
        }
    }

    /// Get the current timestamp.
    fn current_time(&self) -> Timestamp {
        self.config.current_time.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64
        })
    }
}

impl Default for DefaultValidator<NoopPublicKeyLookup, NoopBondChecker> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P, B> Validator for DefaultValidator<P, B>
where
    P: PublicKeyLookup,
    B: BondChecker,
{
    fn validate_content(&self, content: &[u8], manifest: &Manifest) -> ValidationResult<()> {
        validate_content(content, manifest)
    }

    fn validate_version(
        &self,
        manifest: &Manifest,
        previous: Option<&Manifest>,
    ) -> ValidationResult<()> {
        validate_version(manifest, previous)
    }

    fn validate_provenance(
        &self,
        manifest: &Manifest,
        sources: &[Manifest],
    ) -> ValidationResult<()> {
        validate_provenance(manifest, sources)
    }

    fn validate_payment(
        &self,
        payment: &Payment,
        channel: &Channel,
        manifest: &Manifest,
    ) -> ValidationResult<()> {
        // Look up payer's public key for signature verification
        let payer_pubkey = self.pubkey_lookup.lookup(&channel.peer_id);

        // Calculate payment nonce from the payment ID or use channel nonce + 1
        // In practice, the nonce would be derived from the payment structure
        let payment_nonce = channel.nonce + 1;

        validate_payment(
            payment,
            channel,
            manifest,
            payer_pubkey.as_ref(),
            payment_nonce,
        )
    }

    fn validate_message(&self, message: &Message) -> ValidationResult<()> {
        let current_time = self.current_time();
        let sender_pubkey = self.pubkey_lookup.lookup(&message.sender);

        validate_message(message, current_time, sender_pubkey.as_ref())
    }

    fn validate_access(&self, requester: &PeerId, manifest: &Manifest) -> ValidationResult<()> {
        validate_access_with_owner_bypass(requester, manifest, Some(&self.bond_checker))
    }
}

/// No-op public key lookup that always returns None.
#[derive(Clone, Copy, Default)]
pub struct NoopPublicKeyLookup;

impl PublicKeyLookup for NoopPublicKeyLookup {
    fn lookup(&self, _peer_id: &PeerId) -> Option<PublicKey> {
        None
    }
}

/// No-op bond checker that always returns false.
#[derive(Clone, Copy, Default)]
pub struct NoopBondChecker;

impl BondChecker for NoopBondChecker {
    fn has_bond(&self, _peer_id: &PeerId, _amount: u64) -> bool {
        false
    }
}

/// A permissive bond checker that always returns true.
#[derive(Clone, Copy, Default)]
pub struct PermissiveBondChecker;

impl BondChecker for PermissiveBondChecker {
    fn has_bond(&self, _peer_id: &PeerId, _amount: u64) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ValidationError;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use nodalync_types::{Metadata, Visibility};

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    fn create_test_manifest(content: &[u8]) -> Manifest {
        let hash = content_hash(content);
        let owner = test_peer_id();
        let metadata = Metadata::new("Test", content.len() as u64);
        Manifest::new_l0(hash, owner, metadata, 1234567890)
    }

    #[test]
    fn test_default_validator_content() {
        let validator = DefaultValidator::new();
        let content = b"Hello, Nodalync!";
        let manifest = create_test_manifest(content);

        assert!(validator.validate_content(content, &manifest).is_ok());
    }

    #[test]
    fn test_default_validator_version() {
        let validator = DefaultValidator::new();
        let manifest = create_test_manifest(b"Content");

        assert!(validator.validate_version(&manifest, None).is_ok());
    }

    #[test]
    fn test_default_validator_provenance() {
        let validator = DefaultValidator::new();
        let manifest = create_test_manifest(b"Content");

        assert!(validator.validate_provenance(&manifest, &[]).is_ok());
    }

    #[test]
    fn test_default_validator_access() {
        let validator = DefaultValidator::new();
        let mut manifest = create_test_manifest(b"Content");
        manifest.visibility = Visibility::Shared;
        let requester = test_peer_id();

        assert!(validator.validate_access(&requester, &manifest).is_ok());
    }

    #[test]
    fn test_default_validator_access_private() {
        let validator = DefaultValidator::new();
        let manifest = create_test_manifest(b"Content"); // Private by default
        let requester = test_peer_id();

        let result = validator.validate_access(&requester, &manifest);
        assert!(matches!(result, Err(ValidationError::ContentPrivate)));
    }

    #[test]
    fn test_validator_with_fixed_time() {
        let config = ValidatorConfig::new().with_fixed_time(1000000);
        let validator = DefaultValidator::with_config(config);

        assert_eq!(validator.current_time(), 1000000);
    }

    #[test]
    fn test_custom_bond_checker() {
        struct AlwaysHasBond;
        impl BondChecker for AlwaysHasBond {
            fn has_bond(&self, _: &PeerId, _: u64) -> bool {
                true
            }
        }

        let validator = DefaultValidator::with_dependencies(
            ValidatorConfig::default(),
            NoopPublicKeyLookup,
            AlwaysHasBond,
        );

        let mut manifest = create_test_manifest(b"Content");
        manifest.visibility = Visibility::Shared;
        manifest.access.require_bond = true;
        manifest.access.bond_amount = Some(1000);

        let requester = test_peer_id();
        assert!(validator.validate_access(&requester, &manifest).is_ok());
    }

    #[test]
    fn test_validator_trait_object() {
        let validator: Box<dyn Validator> = Box::new(DefaultValidator::new());
        let content = b"Content";
        let manifest = create_test_manifest(content);

        assert!(validator.validate_content(content, &manifest).is_ok());
    }
}
