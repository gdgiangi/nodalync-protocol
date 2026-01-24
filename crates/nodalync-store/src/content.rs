//! Filesystem-based content storage.
//!
//! This module implements content storage using the filesystem.
//! Content is stored in a directory structure organized by hash prefix
//! for efficient file system operations.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use nodalync_crypto::{content_hash, Hash};

use crate::error::{Result, StoreError};
use crate::traits::ContentStore;

/// Filesystem-based content store.
///
/// Stores content in files named by their hash, organized into
/// subdirectories by the first 2 bytes of the hash for efficiency.
///
/// Directory structure:
/// ```text
/// content_dir/
/// ├── ab/
/// │   ├── abcd1234...
/// │   └── abef5678...
/// └── cd/
///     └── cdef9012...
/// ```
pub struct FsContentStore {
    /// Root directory for content storage.
    content_dir: PathBuf,
}

impl FsContentStore {
    /// Create a new filesystem content store.
    ///
    /// Creates the directory if it doesn't exist.
    pub fn new(content_dir: impl AsRef<Path>) -> Result<Self> {
        let content_dir = content_dir.as_ref().to_path_buf();
        fs::create_dir_all(&content_dir)?;
        Ok(Self { content_dir })
    }

    /// Get the path for a content hash.
    ///
    /// Uses the first 2 bytes of the hash as subdirectory name.
    fn content_path(&self, hash: &Hash) -> PathBuf {
        let hex = format!("{}", hash);
        let prefix = &hex[..4]; // First 2 bytes = 4 hex chars
        self.content_dir.join(prefix).join(&hex)
    }

    /// Ensure the parent directory exists for a hash.
    fn ensure_parent_dir(&self, hash: &Hash) -> Result<()> {
        let path = self.content_path(hash);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}

impl ContentStore for FsContentStore {
    fn store(&mut self, content: &[u8]) -> Result<Hash> {
        let hash = content_hash(content);

        // If content already exists, just return the hash
        if self.exists(&hash) {
            return Ok(hash);
        }

        // Ensure parent directory exists
        self.ensure_parent_dir(&hash)?;

        // Write content to file
        let path = self.content_path(&hash);
        let mut file = File::create(&path)?;
        file.write_all(content)?;
        file.sync_all()?;

        Ok(hash)
    }

    fn store_verified(&mut self, hash: &Hash, content: &[u8]) -> Result<()> {
        // Verify hash matches
        let computed = content_hash(content);
        if computed != *hash {
            return Err(StoreError::HashMismatch {
                expected: *hash,
                got: computed,
            });
        }

        // If content already exists, we're done
        if self.exists(hash) {
            return Ok(());
        }

        // Ensure parent directory exists
        self.ensure_parent_dir(hash)?;

        // Write content to file
        let path = self.content_path(hash);
        let mut file = File::create(&path)?;
        file.write_all(content)?;
        file.sync_all()?;

        Ok(())
    }

    fn load(&self, hash: &Hash) -> Result<Option<Vec<u8>>> {
        let path = self.content_path(hash);

        if !path.exists() {
            return Ok(None);
        }

        let mut file = File::open(&path)?;
        let mut content = Vec::new();
        file.read_to_end(&mut content)?;

        Ok(Some(content))
    }

    fn exists(&self, hash: &Hash) -> bool {
        self.content_path(hash).exists()
    }

    fn delete(&mut self, hash: &Hash) -> Result<()> {
        let path = self.content_path(hash);

        if path.exists() {
            fs::remove_file(&path)?;
        }

        // Try to remove parent directory if empty (best effort)
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }

        Ok(())
    }

    fn size(&self, hash: &Hash) -> Result<Option<u64>> {
        let path = self.content_path(hash);

        if !path.exists() {
            return Ok(None);
        }

        let metadata = fs::metadata(&path)?;
        Ok(Some(metadata.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_store_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = FsContentStore::new(temp_dir.path()).unwrap();

        let content = b"Hello, Nodalync!";
        let hash = store.store(content).unwrap();

        let loaded = store.load(&hash).unwrap();
        assert_eq!(loaded, Some(content.to_vec()));
    }

    #[test]
    fn test_store_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = FsContentStore::new(temp_dir.path()).unwrap();

        let content = b"Test content";
        let hash1 = store.store(content).unwrap();
        let hash2 = store.store(content).unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_store_verified() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = FsContentStore::new(temp_dir.path()).unwrap();

        let content = b"Verified content";
        let hash = content_hash(content);

        store.store_verified(&hash, content).unwrap();

        let loaded = store.load(&hash).unwrap();
        assert_eq!(loaded, Some(content.to_vec()));
    }

    #[test]
    fn test_store_verified_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = FsContentStore::new(temp_dir.path()).unwrap();

        let content = b"Some content";
        let wrong_hash = content_hash(b"Different content");

        let result = store.store_verified(&wrong_hash, content);
        assert!(matches!(result, Err(StoreError::HashMismatch { .. })));
    }

    #[test]
    fn test_exists() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = FsContentStore::new(temp_dir.path()).unwrap();

        let content = b"Test";
        let hash = content_hash(content);

        assert!(!store.exists(&hash));

        store.store(content).unwrap();
        assert!(store.exists(&hash));
    }

    #[test]
    fn test_load_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let store = FsContentStore::new(temp_dir.path()).unwrap();

        let hash = content_hash(b"nonexistent");
        let result = store.load(&hash).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = FsContentStore::new(temp_dir.path()).unwrap();

        let content = b"To be deleted";
        let hash = store.store(content).unwrap();
        assert!(store.exists(&hash));

        store.delete(&hash).unwrap();
        assert!(!store.exists(&hash));
    }

    #[test]
    fn test_delete_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = FsContentStore::new(temp_dir.path()).unwrap();

        let hash = content_hash(b"nonexistent");
        // Deleting nonexistent content should not error
        store.delete(&hash).unwrap();
    }

    #[test]
    fn test_size() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = FsContentStore::new(temp_dir.path()).unwrap();

        let content = b"Hello, World!";
        let hash = store.store(content).unwrap();

        let size = store.size(&hash).unwrap();
        assert_eq!(size, Some(content.len() as u64));
    }

    #[test]
    fn test_size_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let store = FsContentStore::new(temp_dir.path()).unwrap();

        let hash = content_hash(b"nonexistent");
        let size = store.size(&hash).unwrap();
        assert!(size.is_none());
    }

    #[test]
    fn test_directory_structure() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = FsContentStore::new(temp_dir.path()).unwrap();

        let content = b"Test content";
        let hash = store.store(content).unwrap();

        // Verify the directory structure
        let hex = format!("{}", hash);
        let prefix = &hex[..4];
        let prefix_dir = temp_dir.path().join(prefix);

        assert!(prefix_dir.exists());
        assert!(prefix_dir.is_dir());
    }

    #[test]
    fn test_multiple_contents_same_prefix() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = FsContentStore::new(temp_dir.path()).unwrap();

        // Store multiple contents - they may or may not share prefix dirs
        let contents: Vec<&[u8]> = vec![b"content1", b"content2", b"content3"];
        let hashes: Vec<Hash> = contents
            .iter()
            .map(|c| store.store(c).unwrap())
            .collect();

        // Verify all can be loaded
        for (content, hash) in contents.iter().zip(hashes.iter()) {
            let loaded = store.load(hash).unwrap();
            assert_eq!(loaded, Some(content.to_vec()));
        }
    }

    #[test]
    fn test_large_content() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = FsContentStore::new(temp_dir.path()).unwrap();

        // 1 MB of data
        let content: Vec<u8> = (0..1024 * 1024).map(|i| (i % 256) as u8).collect();
        let hash = store.store(&content).unwrap();

        let loaded = store.load(&hash).unwrap().unwrap();
        assert_eq!(loaded.len(), content.len());
        assert_eq!(loaded, content);
    }
}
