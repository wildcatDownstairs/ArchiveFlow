use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

use sevenz_rust::Password;

use crate::domain::recovery::AttackMode;
use crate::domain::task::ArchiveType;

use super::generators::{parse_mask, shard_passwords, BruteForceIterator, MaskIterator};
use super::passwords::{
    try_password_7z, try_password_on_archive, try_password_rar, try_password_zip,
    validate_recovery_target,
};
use super::workers::{
    create_result_channel, join_worker_handles, run_stateless_worker_shard, run_zip_worker_shard,
};

fn make_content_encrypted_7z(password: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("hello.txt");
    let archive = dir.path().join("content-encrypted.7z");
    std::fs::write(&source, "secret payload").unwrap();
    sevenz_rust::compress_to_path_encrypted(&source, &archive, Password::from(password)).unwrap();
    (dir, archive)
}

fn zip_fixtures_dir() -> std::path::PathBuf {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap().join("fixtures").join("zip")
}

fn sevenz_fixtures_dir() -> std::path::PathBuf {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap().join("fixtures").join("7z")
}

fn rar_fixtures_dir() -> std::path::PathBuf {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap().join("fixtures").join("rar")
}

fn write_fake_archive(name: &str, bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(bytes).unwrap();
    (dir, path)
}

#[test]
fn bruteforce_abc_1_2_produces_12_items() {
    let items: Vec<String> = BruteForceIterator::new("abc", 1, 2).collect();
    assert_eq!(
        items,
        vec!["a", "b", "c", "aa", "ab", "ac", "ba", "bb", "bc", "ca", "cb", "cc"]
    );
    assert_eq!(items.len(), 12);
}

#[test]
fn bruteforce_ab_1_1_produces_a_b() {
    let items: Vec<String> = BruteForceIterator::new("ab", 1, 1).collect();
    assert_eq!(items, vec!["a", "b"]);
}

#[test]
fn bruteforce_empty_charset_produces_nothing() {
    let items: Vec<String> = BruteForceIterator::new("", 1, 3).collect();
    assert!(items.is_empty());
}

#[test]
fn bruteforce_min_len_zero_treated_as_one() {
    let items: Vec<String> = BruteForceIterator::new("a", 0, 2).collect();
    assert_eq!(items, vec!["a", "aa"]);
}

#[test]
fn bruteforce_max_less_than_min_clamped() {
    let items: Vec<String> = BruteForceIterator::new("ab", 3, 1).collect();
    assert_eq!(items.len(), 8);
    assert_eq!(items[0], "aaa");
    assert_eq!(items[7], "bbb");
}

#[test]
fn bruteforce_skip_to_matches_sequential() {
    let full: Vec<String> = BruteForceIterator::new("abc", 1, 2).collect();
    let mut iter = BruteForceIterator::new("abc", 1, 2);
    iter.skip_to(3);
    let rest: Vec<String> = iter.collect();
    assert_eq!(&full[3..], &rest[..]);
}

#[test]
fn bruteforce_skip_to_zero_is_noop() {
    let full: Vec<String> = BruteForceIterator::new("ab", 1, 2).collect();
    let mut iter = BruteForceIterator::new("ab", 1, 2);
    iter.skip_to(0);
    let rest: Vec<String> = iter.collect();
    assert_eq!(full, rest);
}

#[test]
fn bruteforce_skip_to_past_end_produces_nothing() {
    let mut iter = BruteForceIterator::new("ab", 1, 2);
    iter.skip_to(999);
    assert!(iter.next().is_none());
}

#[test]
fn bruteforce_skip_to_exact_boundary() {
    let full: Vec<String> = BruteForceIterator::new("ab", 1, 2).collect();
    for skip in 0..6 {
        let mut iter = BruteForceIterator::new("ab", 1, 2);
        iter.skip_to(skip);
        let rest: Vec<String> = iter.collect();
        assert_eq!(
            &full[skip as usize..],
            &rest[..],
            "skip_to({}) mismatch",
            skip
        );
    }
}

#[test]
fn shard_passwords_bruteforce_covers_full_space() {
    let mode = AttackMode::BruteForce {
        charset: "ab".to_string(),
        min_length: 1,
        max_length: 2,
    };
    let shard_0: Vec<String> = shard_passwords(&mode, 0, 3).collect();
    let shard_1: Vec<String> = shard_passwords(&mode, 3, 6).collect();
    let full: Vec<String> = BruteForceIterator::new("ab", 1, 2).collect();

    assert_eq!([shard_0, shard_1].concat(), full);
}

#[test]
fn shard_passwords_dictionary_covers_full_space() {
    let mode = AttackMode::Dictionary {
        wordlist: vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
    };
    let shard_0: Vec<String> = shard_passwords(&mode, 0, 2).collect();
    let shard_1: Vec<String> = shard_passwords(&mode, 2, 5).collect();
    assert_eq!(shard_0, vec!["a", "b"]);
    assert_eq!(shard_1, vec!["c", "d", "e"]);
}

#[test]
fn parse_mask_supports_literals_and_tokens() {
    let slots = parse_mask("?dA??").unwrap();
    assert_eq!(slots.len(), 3);
    assert_eq!(slots[0].len(), 10);
    assert_eq!(slots[1], vec!['A']);
    assert_eq!(slots[2], vec!['?']);
}

#[test]
fn mask_iterator_generates_expected_sequence() {
    let items: Vec<String> = MaskIterator::new("?dA").unwrap().collect();
    assert_eq!(items.first().unwrap(), "0A");
    assert_eq!(items.last().unwrap(), "9A");
    assert_eq!(items.len(), 10);
}

#[test]
fn mask_iterator_skip_to_matches_sequence() {
    let full: Vec<String> = MaskIterator::new("?l?d").unwrap().collect();
    let mut iter = MaskIterator::new("?l?d").unwrap();
    iter.skip_to(13);
    let rest: Vec<String> = iter.collect();
    assert_eq!(&full[13..], &rest[..]);
}

#[test]
fn total_combinations_for_mask_is_correct() {
    assert_eq!(MaskIterator::total_combinations("?d?dA").unwrap(), 100);
}

#[test]
fn shard_passwords_mask_covers_full_space() {
    let mode = AttackMode::Mask {
        mask: "?d?d".to_string(),
    };
    let shard_0: Vec<String> = shard_passwords(&mode, 0, 50).collect();
    let shard_1: Vec<String> = shard_passwords(&mode, 50, 100).collect();
    let full: Vec<String> = MaskIterator::new("?d?d").unwrap().collect();
    assert_eq!([shard_0, shard_1].concat(), full);
}

#[test]
fn validate_recovery_target_rejects_corrupt_7z() {
    let (_dir, path) = write_fake_archive(
        "broken.7z",
        &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x00],
    );

    let error = validate_recovery_target(&path, &ArchiveType::SevenZ).unwrap_err();
    assert!(error.contains("无法解析 7z 文件"));
}

#[test]
fn validate_recovery_target_rejects_corrupt_rar() {
    let (_dir, path) = write_fake_archive(
        "broken.rar",
        &[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00, 0x00],
    );

    let error = validate_recovery_target(&path, &ArchiveType::Rar).unwrap_err();
    assert!(error.contains("RAR"));
}

#[test]
fn join_worker_handles_reports_panics() {
    let handle = std::thread::spawn(|| panic!("boom"));
    let panic_messages = join_worker_handles(vec![handle]);

    assert_eq!(panic_messages.len(), 1);
    assert!(panic_messages[0].contains("boom"));
}

#[test]
fn result_channel_buffers_one_hit_per_worker() {
    let (tx, rx) = create_result_channel::<String>(3);

    let handles: Vec<_> = ["pw1", "pw2", "pw3"]
        .into_iter()
        .map(|password| {
            let tx = tx.clone();
            std::thread::spawn(move || {
                tx.send(password.to_string()).unwrap();
            })
        })
        .collect();
    drop(tx);

    let mut results = Vec::new();
    for _ in 0..3 {
        results.push(
            rx.recv_timeout(std::time::Duration::from_secs(1))
                .expect("all worker hits should fit into the result channel"),
        );
    }
    results.sort();

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(results, vec!["pw1", "pw2", "pw3"]);
}

#[test]
fn total_combinations_2_1_2() {
    assert_eq!(BruteForceIterator::total_combinations(2, 1, 2), 6);
}

#[test]
fn total_combinations_26_1_3() {
    assert_eq!(BruteForceIterator::total_combinations(26, 1, 3), 18278);
}

#[test]
fn total_combinations_zero_charset() {
    assert_eq!(BruteForceIterator::total_combinations(0, 1, 3), 0);
}

#[test]
fn total_combinations_min_zero_treated_as_one() {
    assert_eq!(BruteForceIterator::total_combinations(2, 0, 2), 6);
}

#[test]
fn try_password_zip_correct_on_aes() {
    let path = zip_fixtures_dir().join("encrypted-aes.zip");
    assert!(try_password_zip(&path, "test123"));
}

#[test]
fn try_password_zip_wrong_on_aes() {
    let path = zip_fixtures_dir().join("encrypted-aes.zip");
    assert!(!try_password_zip(&path, "wrong"));
}

#[test]
fn try_password_zip_correct_on_strong() {
    let path = zip_fixtures_dir().join("encrypted-strong.zip");
    assert!(try_password_zip(&path, "Str0ng!P@ss"));
}

#[test]
fn try_password_zip_on_unencrypted_returns_false() {
    let path = zip_fixtures_dir().join("normal.zip");
    assert!(!try_password_zip(&path, "anything"));
}

#[test]
fn try_password_zip_nonexistent_file_returns_false() {
    let path = zip_fixtures_dir().join("does-not-exist.zip");
    assert!(!try_password_zip(&path, "test"));
}

#[test]
fn try_password_on_archive_correct() {
    let path = zip_fixtures_dir().join("encrypted-aes.zip");
    let file = std::fs::File::open(&path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();

    let index = (0..archive.len())
        .find(|&i| {
            archive
                .by_index_raw(i)
                .map(|e| e.encrypted() && !e.is_dir())
                .unwrap_or(false)
        })
        .expect("should have encrypted entry");

    assert!(try_password_on_archive(&mut archive, index, "test123"));
}

#[test]
fn try_password_on_archive_wrong() {
    let path = zip_fixtures_dir().join("encrypted-aes.zip");
    let file = std::fs::File::open(&path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();

    let index = (0..archive.len())
        .find(|&i| {
            archive
                .by_index_raw(i)
                .map(|e| e.encrypted() && !e.is_dir())
                .unwrap_or(false)
        })
        .expect("should have encrypted entry");

    assert!(!try_password_on_archive(
        &mut archive,
        index,
        "wrong_password"
    ));
}

#[test]
fn try_password_7z_correct() {
    let path = sevenz_fixtures_dir().join("encrypted.7z");
    assert!(try_password_7z(&path, "test123"));
}

#[test]
fn try_password_7z_wrong() {
    let path = sevenz_fixtures_dir().join("encrypted.7z");
    assert!(!try_password_7z(&path, "wrong_password"));
}

#[test]
fn try_password_7z_on_unencrypted_returns_false() {
    let path = sevenz_fixtures_dir().join("normal.7z");
    assert!(!try_password_7z(&path, "anything"));
}

#[test]
fn try_password_7z_content_encrypted_without_encrypted_headers() {
    let (_dir, path) = make_content_encrypted_7z("test123");
    assert!(try_password_7z(&path, "test123"));
    assert!(!try_password_7z(&path, "wrong_password"));
}

#[test]
fn try_password_7z_nonexistent_file_returns_false() {
    let path = sevenz_fixtures_dir().join("does-not-exist.7z");
    assert!(!try_password_7z(&path, "test"));
}

#[test]
fn try_password_rar_correct() {
    let path = rar_fixtures_dir().join("encrypted.rar");
    assert!(try_password_rar(&path, "unrar"));
}

#[test]
fn try_password_rar_wrong() {
    let path = rar_fixtures_dir().join("encrypted.rar");
    assert!(!try_password_rar(&path, "wrong_password"));
}

#[test]
fn try_password_rar_on_unencrypted_returns_false() {
    let path = rar_fixtures_dir().join("normal.rar");
    assert!(!try_password_rar(&path, "anything"));
}

#[test]
fn try_password_rar_encrypted_headers_correct() {
    let path = rar_fixtures_dir().join("encrypted-headers.rar");
    assert!(try_password_rar(&path, "password"));
}

#[test]
fn try_password_rar_encrypted_headers_wrong() {
    let path = rar_fixtures_dir().join("encrypted-headers.rar");
    assert!(!try_password_rar(&path, "wrong"));
}

#[test]
fn try_password_rar_nonexistent_file_returns_false() {
    let path = rar_fixtures_dir().join("does-not-exist.rar");
    assert!(!try_password_rar(&path, "test"));
}

#[test]
fn parallel_zip_worker_finds_correct_password() {
    let path = zip_fixtures_dir().join("encrypted-aes.zip");
    let cancel = Arc::new(AtomicBool::new(false));
    let counter = Arc::new(AtomicU64::new(0));
    let (tx, rx) = mpsc::sync_channel(1);

    let mode = AttackMode::Dictionary {
        wordlist: vec![
            "wrong1".to_string(),
            "wrong2".to_string(),
            "wrong3".to_string(),
            "test123".to_string(),
            "wrong4".to_string(),
        ],
    };

    run_zip_worker_shard(path, Arc::new(mode), 0, 5, cancel, counter.clone(), tx);

    let found = rx.recv().expect("should find password");
    assert_eq!(found, "test123");
    assert!(counter.load(Ordering::Relaxed) > 0);
}

#[test]
fn parallel_7z_worker_finds_correct_password() {
    let path = sevenz_fixtures_dir().join("encrypted.7z");
    let cancel = Arc::new(AtomicBool::new(false));
    let counter = Arc::new(AtomicU64::new(0));
    let (tx, rx) = mpsc::sync_channel(1);

    let mode = AttackMode::Dictionary {
        wordlist: vec![
            "bad1".to_string(),
            "bad2".to_string(),
            "test123".to_string(),
        ],
    };

    run_stateless_worker_shard(
        path,
        ArchiveType::SevenZ,
        Arc::new(mode),
        0,
        3,
        cancel,
        counter,
        tx,
    );

    let found = rx.recv().expect("should find password");
    assert_eq!(found, "test123");
}

#[test]
fn parallel_rar_worker_finds_correct_password() {
    let path = rar_fixtures_dir().join("encrypted.rar");
    let cancel = Arc::new(AtomicBool::new(false));
    let counter = Arc::new(AtomicU64::new(0));
    let (tx, rx) = mpsc::sync_channel(1);

    let mode = AttackMode::Dictionary {
        wordlist: vec![
            "nope".to_string(),
            "unrar".to_string(),
            "also_nope".to_string(),
        ],
    };

    run_stateless_worker_shard(
        path,
        ArchiveType::Rar,
        Arc::new(mode),
        0,
        3,
        cancel,
        counter,
        tx,
    );

    let found = rx.recv().expect("should find password");
    assert_eq!(found, "unrar");
}

#[test]
fn parallel_worker_respects_cancel_flag() {
    let path = zip_fixtures_dir().join("encrypted-aes.zip");
    let cancel = Arc::new(AtomicBool::new(true));
    let counter = Arc::new(AtomicU64::new(0));
    let (tx, rx) = mpsc::sync_channel(1);

    let mode = AttackMode::Dictionary {
        wordlist: vec!["test123".to_string()],
    };

    run_zip_worker_shard(path, Arc::new(mode), 0, 1, cancel, counter, tx);

    assert!(rx.try_recv().is_err());
}

#[test]
fn parallel_multi_worker_zip_finds_password() {
    let path = zip_fixtures_dir().join("encrypted-aes.zip");
    let cancel = Arc::new(AtomicBool::new(false));
    let counter = Arc::new(AtomicU64::new(0));
    let (tx, rx) = mpsc::sync_channel(1);

    let words = vec![
        "w1".to_string(),
        "w2".to_string(),
        "w3".to_string(),
        "w4".to_string(),
        "w5".to_string(),
        "test123".to_string(),
    ];
    let mode = Arc::new(AttackMode::Dictionary { wordlist: words });

    let mut handles = vec![];
    for (s, e) in [(0u64, 2u64), (2, 4), (4, 6)] {
        let p = path.clone();
        let m = Arc::clone(&mode);
        let c = Arc::clone(&cancel);
        let t = Arc::clone(&counter);
        let tx2 = tx.clone();
        handles.push(std::thread::spawn(move || {
            run_zip_worker_shard(p, m, s, e, c, t, tx2);
        }));
    }
    drop(tx);

    let found = rx.recv().expect("some worker should find password");
    assert_eq!(found, "test123");
    cancel.store(true, Ordering::Relaxed);
    for h in handles {
        let _ = h.join();
    }
}
