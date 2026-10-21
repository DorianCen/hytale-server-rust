//! `IndexedStorageFile` layout and behavior tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use hytale_storage::indexed_storage_file::{
    IndexedStorageFile, BLOB_HEADER_LENGTH, HEADER_LENGTH, INDEX_SIZE, MAGIC, VERSION,
};

fn temp_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-storage-{}-{nonce}-{name}.bin",
        std::process::id()
    ))
}

fn lcg_bytes(length: usize) -> Vec<u8> {
    let mut state = 0x1234_5678_9ABC_DEF0u64;
    (0..length)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as u8
        })
        .collect()
}

#[test]
fn header_and_index_layout_exactly() {
    let path = temp_path("layout");
    let mut file = IndexedStorageFile::create(&path, 4, 64).expect("create");
    assert_eq!(file.size().unwrap(), HEADER_LENGTH + 4 * INDEX_SIZE);
    assert_eq!(file.version(), VERSION);
    assert_eq!(file.blob_count(), 4);
    assert_eq!(file.segment_size(), 64);
    assert_eq!(file.used_blob_count(), 0);
    assert!(file.keys().is_empty());

    let bytes = fs::read(&path).unwrap();
    assert_eq!(&bytes[..20], MAGIC);
    assert_eq!(
        u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
        VERSION
    );
    assert_eq!(u32::from_be_bytes(bytes[24..28].try_into().unwrap()), 4);
    assert_eq!(u32::from_be_bytes(bytes[28..32].try_into().unwrap()), 64);
    assert!(bytes[32..48].iter().all(|byte| *byte == 0));

    file.write_blob(2, b"hello indexed storage").unwrap();
    assert_eq!(file.used_blob_count(), 1);
    assert_eq!(file.keys(), vec![2]);
    let bytes = fs::read(&path).unwrap();
    let first = i32::from_be_bytes(bytes[32 + 2 * 4..32 + 3 * 4].try_into().unwrap());
    assert_eq!(first, 1);
    let base = (HEADER_LENGTH + 4 * INDEX_SIZE) as usize;
    let source = u32::from_be_bytes(bytes[base..base + 4].try_into().unwrap());
    let compressed = u32::from_be_bytes(bytes[base + 4..base + 8].try_into().unwrap());
    assert_eq!(source, b"hello indexed storage".len() as u32);
    assert!(compressed > 0);
    let frame = &bytes[base + BLOB_HEADER_LENGTH as usize..];
    let decoded = zstd::bulk::decompress(&frame[..compressed as usize], source as usize).unwrap();
    assert_eq!(decoded, b"hello indexed storage");
    fs::remove_file(path).ok();
}

#[test]
fn write_read_overwrite_remove_and_reopen() {
    let path = temp_path("roundtrip");
    {
        let mut file = IndexedStorageFile::create(&path, 16, 64).expect("create");
        let large = lcg_bytes(10_000);
        file.write_blob(0, b"").expect("empty blob");
        file.write_blob(1, &large).expect("large blob");
        file.write_blob(7, b"seven").expect("small blob");
        assert_eq!(file.read_blob(0).unwrap(), Some(Vec::new()));
        assert_eq!(file.read_blob(1).unwrap(), Some(large.clone()));
        assert_eq!(file.read_blob(7).unwrap(), Some(b"seven".to_vec()));
        assert_eq!(file.read_blob(2).unwrap(), None);
        assert_eq!(file.used_blob_count(), 3);
        assert_eq!(file.keys(), vec![0, 1, 7]);
        assert!(file.used_segment_count() >= 10_000 / 64);

        // Overwrite reuses the slot and frees the old segments.
        let replacement = lcg_bytes(2_000);
        file.write_blob(1, &replacement).expect("overwrite");
        assert_eq!(file.used_blob_count(), 3);
        assert_eq!(file.read_blob(1).unwrap(), Some(replacement));
        file.flush().unwrap();
    }

    {
        let mut file = IndexedStorageFile::open(&path, false).expect("reopen");
        assert_eq!(file.used_blob_count(), 3);
        assert_eq!(file.keys(), vec![0, 1, 7]);
        assert_eq!(file.read_blob(1).unwrap().unwrap().len(), 2_000);

        file.remove_blob(1).unwrap();
        assert_eq!(file.read_blob(1).unwrap(), None);
        assert_eq!(file.used_blob_count(), 2);
        assert_eq!(file.keys(), vec![0, 7]);
        file.remove_blob(1).unwrap(); // Removing an empty slot is a no-op.
        assert_eq!(file.used_blob_count(), 2);
    }

    {
        let mut file = IndexedStorageFile::open(&path, true).expect("read-only reopen");
        assert_eq!(file.read_blob(7).unwrap(), Some(b"seven".to_vec()));
        assert!(file.write_blob(3, b"nope").is_err());
        assert!(file.remove_blob(7).is_err());
    }
    fs::remove_file(path).ok();
}

#[test]
fn blob_spanning_many_segments() {
    let path = temp_path("segments");
    let payload = lcg_bytes(64 * 40 + 17);
    {
        let mut file = IndexedStorageFile::create(&path, 8, 64).unwrap();
        file.write_blob(3, &payload).unwrap();
        assert_eq!(file.read_blob(3).unwrap(), Some(payload.clone()));
        assert!(file.used_segment_count() >= payload.len() / 64);
        // A second blob starts after the first contiguous run.
        file.write_blob(4, b"next").unwrap();
        assert_eq!(file.read_blob(4).unwrap(), Some(b"next".to_vec()));
    }
    {
        let mut file = IndexedStorageFile::open(&path, false).unwrap();
        assert_eq!(file.read_blob(3).unwrap(), Some(payload));
    }
    fs::remove_file(path).ok();
}

#[test]
fn first_fit_reuses_freed_run() {
    let path = temp_path("reuse");
    let mut file = IndexedStorageFile::create(&path, 8, 64).unwrap();
    let payload = lcg_bytes(200);
    file.write_blob(0, &payload).unwrap();
    let first_before = file.keys();
    assert_eq!(first_before, vec![0]);
    let bytes = fs::read(&path).unwrap();
    let first = i32::from_be_bytes(bytes[32..36].try_into().unwrap());
    assert_eq!(first, 1);

    file.remove_blob(0).unwrap();
    file.write_blob(1, b"replacement").unwrap();
    let bytes = fs::read(&path).unwrap();
    let reused = i32::from_be_bytes(bytes[32 + 4..32 + 8].try_into().unwrap());
    assert_eq!(reused, 1, "first-fit should reuse the freed run");
    fs::remove_file(path).ok();
}

#[test]
fn invalid_files_are_rejected() {
    let path = temp_path("bad-magic");
    fs::write(&path, vec![0u8; 64]).unwrap();
    assert!(IndexedStorageFile::open(&path, true).is_err());
    fs::remove_file(&path).ok();

    let path = temp_path("bad-version");
    IndexedStorageFile::create(&path, 4, 64).unwrap();
    let mut bytes = fs::read(&path).unwrap();
    bytes[20..24].copy_from_slice(&2u32.to_be_bytes());
    fs::write(&path, &bytes).unwrap();
    assert!(IndexedStorageFile::open(&path, true).is_err());
    fs::remove_file(&path).ok();

    let path = temp_path("truncated");
    fs::write(&path, MAGIC).unwrap();
    assert!(IndexedStorageFile::open(&path, true).is_err());
    fs::remove_file(&path).ok();
}

#[test]
fn index_is_bounds_checked() {
    let path = temp_path("bounds");
    let mut file = IndexedStorageFile::create(&path, 2, 64).unwrap();
    assert!(file.write_blob(2, b"x").is_err());
    assert!(file.read_blob(99).is_err());
    fs::remove_file(path).ok();
}

#[test]
fn java_region_fixture_is_read_if_present() {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/golden/server/storage");
    if !directory.is_dir() {
        eprintln!(
            "skipping: no Java storage fixture at {}",
            directory.display()
        );
        return;
    }
    let mut found = false;
    for entry in fs::read_dir(&directory).expect("fixture directory") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        found = true;
        let mut file = IndexedStorageFile::open(&path, true)
            .unwrap_or_else(|error| panic!("open {}: {error}", path.display()));
        assert_eq!(file.keys(), vec![0, 1, 5]);
        for key in file.keys() {
            let blob = file
                .read_blob(key)
                .unwrap_or_else(|error| panic!("read {} blob {key}: {error}", path.display()));
            assert!(blob.is_some());
        }
        if path.file_name().and_then(|name| name.to_str()) == Some("java-sample.region.bin") {
            assert_eq!(file.read_blob(0).unwrap(), Some(Vec::new()));
            assert_eq!(
                file.read_blob(1).unwrap(),
                Some(b"hello java indexed storage".to_vec())
            );
            assert_eq!(file.read_blob(5).unwrap(), Some(lcg_bytes(1_000)));
        }
    }
    if !found {
        eprintln!("skipping: fixture directory has no .bin files");
    }
}
