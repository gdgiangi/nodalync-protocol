//! Access validation (§9.6).
//!
//! This module validates access permissions:
//! - Visibility checks (Private, Unlisted, Shared)
//! - Allowlist/denylist enforcement
//! - Bond requirements

use nodalync_types::{Manifest, PeerId, Visibility};

use crate::error::{ValidationError, ValidationResult};
use crate::payment::BondChecker;

/// Validate access for a requester to content.
///
/// Checks all access validation rules from §9.6:
///
/// - **Private**: Always deny external access
/// - **Unlisted**: Check allowlist (if set), then denylist
/// - **Shared**: Check denylist only (allowlist ignored)
/// - If `require_bond` is true, verify the requester has posted the required bond
///
/// # Arguments
///
/// * `requester` - The peer requesting access
/// * `manifest` - The manifest for the content
/// * `bond_checker` - Optional bond checker for verifying bonds
///
/// # Returns
///
/// `Ok(())` if access is granted, or `Err(ValidationError)`.
pub fn validate_access(
    requester: &PeerId,
    manifest: &Manifest,
    bond_checker: Option<&dyn BondChecker>,
) -> ValidationResult<()> {
    // Check visibility rules
    match manifest.visibility {
        Visibility::Private => {
            // Private content is never accessible externally
            return Err(ValidationError::ContentPrivate);
        }
        Visibility::Unlisted => {
            // Check allowlist if set
            if let Some(ref allowlist) = manifest.access.allowlist {
                if !allowlist.contains(requester) {
                    return Err(ValidationError::NotInAllowlist);
                }
            }
            // Check denylist if set
            if let Some(ref denylist) = manifest.access.denylist {
                if denylist.contains(requester) {
                    return Err(ValidationError::InDenylist);
                }
            }
        }
        Visibility::Shared => {
            // For Shared, allowlist is ignored, only check denylist
            if let Some(ref denylist) = manifest.access.denylist {
                if denylist.contains(requester) {
                    return Err(ValidationError::InDenylist);
                }
            }
        }
        // Handle future visibility variants conservatively (deny by default)
        _ => {
            return Err(ValidationError::ContentPrivate);
        }
    }

    // Check bond requirement
    if manifest.access.require_bond {
        let required_amount = manifest.access.bond_amount.unwrap_or(0);
        if required_amount > 0 {
            if let Some(checker) = bond_checker {
                if !checker.has_bond(requester, required_amount) {
                    return Err(ValidationError::BondRequired {
                        required: required_amount,
                    });
                }
            } else {
                // No bond checker provided but bond is required
                return Err(ValidationError::BondRequired {
                    required: required_amount,
                });
            }
        }
    }

    Ok(())
}

/// Validate access without bond checking.
///
/// Use when bond checking is not required or bonds are checked separately.
pub fn validate_access_basic(requester: &PeerId, manifest: &Manifest) -> ValidationResult<()> {
    validate_access(requester, manifest, None)
}

/// Check if a peer is the owner of the content.
///
/// Owners always have access to their own content.
pub fn is_owner(requester: &PeerId, manifest: &Manifest) -> bool {
    *requester == manifest.owner
}

/// Validate access, allowing owner access regardless of visibility.
///
/// The owner of content can always access it, even if it's private.
pub fn validate_access_with_owner_bypass(
    requester: &PeerId,
    manifest: &Manifest,
    bond_checker: Option<&dyn BondChecker>,
) -> ValidationResult<()> {
    // Owner always has access
    if is_owner(requester, manifest) {
        return Ok(());
    }

    validate_access(requester, manifest, bond_checker)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};
    use nodalync_types::{AccessControl, Metadata};

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    fn create_test_manifest(visibility: Visibility) -> Manifest {
        let hash = content_hash(b"test content");
        let owner = test_peer_id();
        let metadata = Metadata::new("Test", 12);
        let mut manifest = Manifest::new_l0(hash, owner, metadata, 1234567890);
        manifest.visibility = visibility;
        manifest
    }

    struct MockBondChecker {
        has_bond: bool,
    }

    impl BondChecker for MockBondChecker {
        fn has_bond(&self, _peer_id: &PeerId, _amount: u64) -> bool {
            self.has_bond
        }
    }

    #[test]
    fn test_private_always_denied() {
        let manifest = create_test_manifest(Visibility::Private);
        let requester = test_peer_id();

        let result = validate_access_basic(&requester, &manifest);
        assert!(matches!(result, Err(ValidationError::ContentPrivate)));
    }

    #[test]
    fn test_shared_allowed_by_default() {
        let manifest = create_test_manifest(Visibility::Shared);
        let requester = test_peer_id();

        let result = validate_access_basic(&requester, &manifest);
        assert!(result.is_ok());
    }

    #[test]
    fn test_unlisted_allowed_by_default() {
        let manifest = create_test_manifest(Visibility::Unlisted);
        let requester = test_peer_id();

        let result = validate_access_basic(&requester, &manifest);
        assert!(result.is_ok());
    }

    #[test]
    fn test_unlisted_with_allowlist() {
        let mut manifest = create_test_manifest(Visibility::Unlisted);
        let allowed_peer = test_peer_id();
        let other_peer = test_peer_id();

        manifest.access = AccessControl::with_allowlist(vec![allowed_peer]);

        // Allowed peer can access
        assert!(validate_access_basic(&allowed_peer, &manifest).is_ok());

        // Other peer cannot
        let result = validate_access_basic(&other_peer, &manifest);
        assert!(matches!(result, Err(ValidationError::NotInAllowlist)));
    }

    #[test]
    fn test_unlisted_with_denylist() {
        let mut manifest = create_test_manifest(Visibility::Unlisted);
        let blocked_peer = test_peer_id();
        let other_peer = test_peer_id();

        manifest.access = AccessControl::with_denylist(vec![blocked_peer]);

        // Blocked peer cannot access
        let result = validate_access_basic(&blocked_peer, &manifest);
        assert!(matches!(result, Err(ValidationError::InDenylist)));

        // Other peer can access
        assert!(validate_access_basic(&other_peer, &manifest).is_ok());
    }

    #[test]
    fn test_shared_ignores_allowlist() {
        let mut manifest = create_test_manifest(Visibility::Shared);
        let allowed_peer = test_peer_id();
        let other_peer = test_peer_id();

        // Set allowlist (should be ignored for Shared)
        manifest.access = AccessControl::with_allowlist(vec![allowed_peer]);

        // Both peers can access (allowlist ignored)
        assert!(validate_access_basic(&allowed_peer, &manifest).is_ok());
        assert!(validate_access_basic(&other_peer, &manifest).is_ok());
    }

    #[test]
    fn test_shared_checks_denylist() {
        let mut manifest = create_test_manifest(Visibility::Shared);
        let blocked_peer = test_peer_id();
        let other_peer = test_peer_id();

        manifest.access = AccessControl::with_denylist(vec![blocked_peer]);

        // Blocked peer cannot access
        let result = validate_access_basic(&blocked_peer, &manifest);
        assert!(matches!(result, Err(ValidationError::InDenylist)));

        // Other peer can access
        assert!(validate_access_basic(&other_peer, &manifest).is_ok());
    }

    #[test]
    fn test_allowlist_and_denylist_combined() {
        let mut manifest = create_test_manifest(Visibility::Unlisted);
        let allowed_peer = test_peer_id();
        let blocked_peer = test_peer_id();
        let both_peer = test_peer_id();

        manifest.access.allowlist = Some(vec![allowed_peer, both_peer]);
        manifest.access.denylist = Some(vec![blocked_peer, both_peer]);

        // Allowed but not blocked: OK
        assert!(validate_access_basic(&allowed_peer, &manifest).is_ok());

        // Blocked: denied
        let result = validate_access_basic(&blocked_peer, &manifest);
        assert!(matches!(result, Err(ValidationError::NotInAllowlist)));

        // Both allowed and blocked: denylist takes precedence
        let result = validate_access_basic(&both_peer, &manifest);
        assert!(matches!(result, Err(ValidationError::InDenylist)));
    }

    #[test]
    fn test_bond_required_with_checker() {
        let mut manifest = create_test_manifest(Visibility::Shared);
        manifest.access.require_bond = true;
        manifest.access.bond_amount = Some(1000);

        let requester = test_peer_id();

        // With bond
        let checker = MockBondChecker { has_bond: true };
        assert!(validate_access(&requester, &manifest, Some(&checker)).is_ok());

        // Without bond
        let checker = MockBondChecker { has_bond: false };
        let result = validate_access(&requester, &manifest, Some(&checker));
        assert!(matches!(
            result,
            Err(ValidationError::BondRequired { required: 1000 })
        ));
    }

    #[test]
    fn test_bond_required_no_checker() {
        let mut manifest = create_test_manifest(Visibility::Shared);
        manifest.access.require_bond = true;
        manifest.access.bond_amount = Some(1000);

        let requester = test_peer_id();

        // No checker provided but bond required
        let result = validate_access(&requester, &manifest, None);
        assert!(matches!(
            result,
            Err(ValidationError::BondRequired { required: 1000 })
        ));
    }

    #[test]
    fn test_bond_not_required() {
        let manifest = create_test_manifest(Visibility::Shared);
        let requester = test_peer_id();

        // Bond not required, should pass
        assert!(validate_access_basic(&requester, &manifest).is_ok());
    }

    #[test]
    fn test_is_owner() {
        let manifest = create_test_manifest(Visibility::Private);
        let owner = manifest.owner;
        let other = test_peer_id();

        assert!(is_owner(&owner, &manifest));
        assert!(!is_owner(&other, &manifest));
    }

    #[test]
    fn test_owner_bypass_private() {
        let manifest = create_test_manifest(Visibility::Private);
        let owner = manifest.owner;
        let other = test_peer_id();

        // Owner can access private content
        assert!(validate_access_with_owner_bypass(&owner, &manifest, None).is_ok());

        // Others cannot
        let result = validate_access_with_owner_bypass(&other, &manifest, None);
        assert!(matches!(result, Err(ValidationError::ContentPrivate)));
    }

    #[test]
    fn test_owner_bypass_with_denylist() {
        let mut manifest = create_test_manifest(Visibility::Shared);
        let owner = manifest.owner;

        // Add owner to denylist
        manifest.access = AccessControl::with_denylist(vec![owner]);

        // Owner still has access (owner bypass)
        assert!(validate_access_with_owner_bypass(&owner, &manifest, None).is_ok());
    }
}
