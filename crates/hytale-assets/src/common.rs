//! Common assets: the files the client must download before setup completes.
//!
//! Java reads `CommonAssetsIndex.hashes` at the pack root, keeps the hash and
//! pack-relative name of every file under `Common/`, advertises them in
//! `WorldSettings.requiredAssets`, and answers `RequestAssets` with
//! `AssetInitialize`/`AssetPart`/`AssetFinalize` blobs. This module ports the
//! metadata and blob-loading half; the session layer owns the packet stream.
//!
//! The index is optional: a pack without `CommonAssetsIndex.hashes` simply has
//! no common assets. Blob bytes are loaded lazily from the pack and verified
//! against their SHA-256 hash before they are sent.

use std::collections::HashMap;

use hytale_protocol::generated::common::Asset;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::pack::{AssetError, AssetPack};

/// Index file at the pack root.
pub const COMMON_ASSETS_INDEX: &str = "CommonAssetsIndex.hashes";
/// Directory holding the files named by the index.
pub const COMMON_ASSETS_ROOT: &str = "Common";
/// Version prefix accepted on the first index line.
const INDEX_VERSION_PREFIX: &str = "VERSION=";

/// Errors from common-asset loading.
#[derive(Debug, Error)]
pub enum CommonAssetError {
    /// Pack access failed.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// A hash in the index is not 64 hexadecimal characters.
    #[error("{index}:{line}: invalid SHA-256 hash `{hash}`")]
    InvalidHash {
        /// Index file name.
        index: &'static str,
        /// One-based line number.
        line: usize,
        /// Offending value.
        hash: String,
    },
    /// An index entry has no usable relative path.
    #[error("{index}:{line}: invalid asset path `{name}`")]
    InvalidPath {
        /// Index file name.
        index: &'static str,
        /// One-based line number.
        line: usize,
        /// Offending value.
        name: String,
    },
    /// The index declares a version newer than this port understands.
    #[error("{index}: unsupported version {version}")]
    UnsupportedVersion {
        /// Index file name.
        index: &'static str,
        /// Version value.
        version: u32,
    },
    /// A requested hash is not in the index.
    #[error("requested common asset hash `{0}` is unknown")]
    UnknownHash(String),
    /// The bytes on disk do not match the index hash.
    #[error("common asset `{name}` hash mismatch: expected {expected}, got {actual}")]
    HashMismatch {
        /// Pack-relative name.
        name: String,
        /// Index hash.
        expected: String,
        /// Actual SHA-256.
        actual: String,
    },
}

/// Result alias for common-asset loading.
pub type Result<T> = std::result::Result<T, CommonAssetError>;

/// One indexed common asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonAssetInfo {
    /// SHA-256 of the file contents, lowercase hexadecimal.
    pub hash: String,
    /// Pack-relative name below `Common/`.
    pub name: String,
}

impl CommonAssetInfo {
    /// The protocol `Asset` entry advertised in `WorldSettings.requiredAssets`.
    pub fn to_packet(&self) -> Asset {
        Asset {
            hash: self.hash.clone(),
            name: self.name.clone(),
        }
    }

    /// Full pack path of the blob.
    pub fn pack_path(&self) -> String {
        format!("{COMMON_ASSETS_ROOT}/{}", self.name)
    }
}

/// The common-asset index of one pack.
#[derive(Debug, Clone)]
pub struct CommonAssetRegistry {
    pack: AssetPack,
    assets: Vec<CommonAssetInfo>,
    by_hash: HashMap<String, usize>,
    by_name: HashMap<String, usize>,
}

impl CommonAssetRegistry {
    /// Read `CommonAssetsIndex.hashes` from `pack`.
    ///
    /// A pack without the index has an empty registry.
    pub fn load(pack: &AssetPack) -> Result<Self> {
        let index = match pack.read(COMMON_ASSETS_INDEX) {
            Ok(bytes) => bytes,
            Err(AssetError::NotFound(_)) => Vec::new(),
            Err(AssetError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                Vec::new()
            }
            Err(error) => return Err(error.into()),
        };
        Self::from_index(pack.clone(), &index)
    }

    /// Build a registry from raw index bytes (used by tests).
    pub fn from_index(pack: AssetPack, index: &[u8]) -> Result<Self> {
        let text = String::from_utf8_lossy(index);
        let mut assets: Vec<CommonAssetInfo> = Vec::new();
        let mut by_name: HashMap<String, usize> = HashMap::new();

        for (zero_line, raw_line) in text.lines().enumerate() {
            let line = zero_line + 1;
            let raw_line = raw_line.trim_end_matches('\r');
            if raw_line.is_empty() {
                continue;
            }
            if let Some(version) = raw_line.strip_prefix(INDEX_VERSION_PREFIX) {
                let version =
                    version
                        .parse::<u32>()
                        .map_err(|_| CommonAssetError::InvalidPath {
                            index: COMMON_ASSETS_INDEX,
                            line,
                            name: raw_line.to_string(),
                        })?;
                if version > 0 {
                    return Err(CommonAssetError::UnsupportedVersion {
                        index: COMMON_ASSETS_INDEX,
                        version,
                    });
                }
                continue;
            }
            let Some((hash, name)) = raw_line.split_once(' ') else {
                // Java logs and skips corrupt lines.
                continue;
            };
            let hash = hash.to_ascii_lowercase();
            if !is_sha256_hex(&hash) {
                return Err(CommonAssetError::InvalidHash {
                    index: COMMON_ASSETS_INDEX,
                    line,
                    hash,
                });
            }
            if !valid_relative_path(name) {
                return Err(CommonAssetError::InvalidPath {
                    index: COMMON_ASSETS_INDEX,
                    line,
                    name: name.to_string(),
                });
            }
            let info = CommonAssetInfo {
                hash,
                name: name.to_string(),
            };
            // Java keys the registry by name and keeps the last pack entry.
            if let Some(index) = by_name.get(name).copied() {
                assets[index] = info;
            } else {
                let index = assets.len();
                by_name.insert(name.to_string(), index);
                assets.push(info);
            }
        }

        let mut by_hash = HashMap::with_capacity(assets.len());
        for (index, asset) in assets.iter().enumerate() {
            // Duplicate content is normal (the same texture in several paths).
            by_hash.entry(asset.hash.clone()).or_insert(index);
        }

        Ok(Self {
            pack,
            assets,
            by_hash,
            by_name,
        })
    }

    /// All advertised assets, in pack/index order.
    pub fn assets(&self) -> &[CommonAssetInfo] {
        &self.assets
    }

    /// Number of advertised assets.
    pub fn len(&self) -> usize {
        self.assets.len()
    }

    /// Whether the pack has no common assets.
    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }

    /// Look up an asset by hash.
    pub fn get_by_hash(&self, hash: &str) -> Option<&CommonAssetInfo> {
        self.by_hash
            .get(&hash.to_ascii_lowercase())
            .map(|index| &self.assets[*index])
    }

    /// Look up an asset by pack-relative name.
    pub fn get_by_name(&self, name: &str) -> Option<&CommonAssetInfo> {
        self.by_name.get(name).map(|index| &self.assets[*index])
    }

    /// The `WorldSettings.requiredAssets` payload.
    pub fn required_assets(&self) -> Vec<Asset> {
        self.assets.iter().map(CommonAssetInfo::to_packet).collect()
    }

    /// Resolve a `RequestAssets.assets` payload.
    ///
    /// `None` means "everything"; `Some` entries are matched by hash and an
    /// unknown hash is an error (Java disconnects the client).
    pub fn requested(&self, requested: Option<&[Asset]>) -> Result<Vec<&CommonAssetInfo>> {
        match requested {
            None => Ok(self.assets.iter().collect()),
            Some(assets) => assets
                .iter()
                .map(|asset| {
                    self.get_by_hash(&asset.hash)
                        .ok_or_else(|| CommonAssetError::UnknownHash(asset.hash.clone()))
                })
                .collect(),
        }
    }

    /// Load and verify the blob for one indexed asset.
    pub fn load_blob(&self, asset: &CommonAssetInfo) -> Result<Vec<u8>> {
        let bytes = self.pack.read(&asset.pack_path())?;
        verify_hash(asset, &bytes)?;
        Ok(bytes)
    }

    /// Load and verify several blobs, opening a zip pack once.
    pub fn load_blobs(&self, assets: &[&CommonAssetInfo]) -> Result<Vec<Vec<u8>>> {
        let paths: Vec<String> = assets.iter().map(|asset| asset.pack_path()).collect();
        let blobs = self.pack.read_many(&paths)?;
        for (asset, bytes) in assets.iter().zip(&blobs) {
            verify_hash(asset, bytes)?;
        }
        Ok(blobs)
    }

    /// The pack the registry was read from.
    pub fn pack(&self) -> &AssetPack {
        &self.pack
    }
}

fn verify_hash(asset: &CommonAssetInfo, bytes: &[u8]) -> Result<()> {
    let actual = hex(Sha256::digest(bytes).as_slice());
    if actual != asset.hash {
        return Err(CommonAssetError::HashMismatch {
            name: asset.name.clone(),
            expected: asset.hash.clone(),
            actual,
        });
    }
    Ok(())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_relative_path(name: &str) -> bool {
    if name.is_empty() || name.starts_with('/') || name.contains('\\') {
        return false;
    }
    name.split('/')
        .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use sha2::{Digest, Sha256};

    use super::*;

    fn temp_pack(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "hytale-common-{}-{nonce}-{name}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("Common/Blocks")).expect("create pack");
        std::fs::write(root.join("Common/Blocks/Stone.png"), b"stone").expect("write blob");
        root
    }

    fn index_for(bytes: &[u8]) -> String {
        let hash = hex(Sha256::digest(bytes).as_slice());
        format!("VERSION=0\n{hash} Blocks/Stone.png\n")
    }

    #[test]
    fn parses_index_and_returns_protocol_assets() {
        let root = temp_pack("parse");
        std::fs::write(root.join(COMMON_ASSETS_INDEX), index_for(b"stone")).expect("index");
        let pack = AssetPack::open(&root).expect("pack");
        let registry = CommonAssetRegistry::load(&pack).expect("registry");
        assert_eq!(registry.len(), 1);
        let asset = &registry.required_assets()[0];
        assert_eq!(asset.name, "Blocks/Stone.png");
        assert_eq!(asset.hash.len(), 64);
        assert_eq!(registry.requested(None).expect("all").len(), 1);
        let blob = registry
            .load_blob(registry.get_by_hash(&asset.hash).expect("by hash"))
            .expect("blob");
        assert_eq!(blob, b"stone");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rejects_unknown_hashes_and_hash_mismatches() {
        let root = temp_pack("reject");
        std::fs::write(root.join(COMMON_ASSETS_INDEX), index_for(b"stone")).expect("index");
        let pack = AssetPack::open(&root).expect("pack");
        let registry = CommonAssetRegistry::load(&pack).expect("registry");
        assert!(matches!(
            registry.requested(Some(&[Asset {
                hash: "0".repeat(64),
                name: "Blocks/Missing.png".to_string(),
            }])),
            Err(CommonAssetError::UnknownHash(_))
        ));

        std::fs::write(root.join("Common/Blocks/Stone.png"), b"changed").expect("blob");
        assert!(matches!(
            registry.load_blob(&registry.assets()[0]),
            Err(CommonAssetError::HashMismatch { .. })
        ));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn empty_pack_has_no_common_assets() {
        let root = temp_pack("empty");
        let pack = AssetPack::open(&root).expect("pack");
        let registry = CommonAssetRegistry::load(&pack).expect("registry");
        assert!(registry.is_empty());
        std::fs::remove_dir_all(root).ok();
    }
}
