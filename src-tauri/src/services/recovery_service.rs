use std::io::{Read, Seek};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use tauri::Emitter;

use crate::domain::recovery::{AttackMode, RecoveryConfig, RecoveryProgress, RecoveryStatus};

// ─── 密码验证 ─────────────────────────────────────────────────────

/// 在已打开的 ZipArchive 上，用给定密码尝试解密指定索引的条目。
/// 利用复用的 archive 避免每次密码尝试都重新打开文件。
/// 返回 true 表示密码正确，false 表示密码错误。
pub fn try_password_on_archive<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    index: usize,
    password: &str,
) -> bool {
    // 尝试用密码解密
    let result = archive.by_index_decrypt(index, password.as_bytes());
    let mut zip_file = match result {
        Ok(f) => f,
        Err(_) => return false, // InvalidPassword 或其他 IO 错误
    };

    // by_index_decrypt 成功不代表密码一定正确（ZipCrypto 有 1/256 误判率）
    // 需要实际读取全部数据，如果 CRC 校验失败会返回 IO 错误
    let mut buf = Vec::new();
    match zip_file.read_to_end(&mut buf) {
        Ok(_) => true,
        Err(_) => false, // CRC 校验失败 → 密码错误
    }
}

/// 独立版本：尝试用给定密码打开 ZIP 文件中的第一个加密条目。
/// 每次调用都会重新打开文件（适合单次测试，不适合热路径）。
#[allow(dead_code)]
pub fn try_password_zip(file_path: &Path, password: &str) -> bool {
    let file = match std::fs::File::open(file_path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(_) => return false,
    };

    // 找到第一个加密的非目录条目的索引
    let encrypted_index = (0..archive.len()).find(|&i| {
        archive
            .by_index_raw(i)
            .map(|entry| entry.encrypted() && !entry.is_dir())
            .unwrap_or(false)
    });

    let index = match encrypted_index {
        Some(i) => i,
        None => return false, // 没有加密条目
    };

    try_password_on_archive(&mut archive, index, password)
}

// ─── 暴力破解密码生成器 ────────────────────────────────────────────

/// 暴力破解密码迭代器：生成指定字符集在 [min_len, max_len] 范围内的所有组合
pub struct BruteForceIterator {
    charset: Vec<char>,
    max_len: usize,
    current_len: usize,
    /// 当前位置的索引数组（每个元素是 charset 中的索引）
    indices: Vec<usize>,
    /// 当前长度是否已穷尽
    exhausted: bool,
    /// 整体是否完成
    done: bool,
}

impl BruteForceIterator {
    pub fn new(charset: &str, min_len: usize, max_len: usize) -> Self {
        let chars: Vec<char> = charset.chars().collect();
        let actual_min = if min_len == 0 { 1 } else { min_len };
        let actual_max = if max_len < actual_min {
            actual_min
        } else {
            max_len
        };

        Self {
            charset: chars,
            max_len: actual_max,
            current_len: actual_min,
            indices: vec![0; actual_min],
            exhausted: false,
            done: false,
        }
    }

    /// 计算总组合数（用于进度展示）
    pub fn total_combinations(charset_len: usize, min_len: usize, max_len: usize) -> u64 {
        let actual_min = if min_len == 0 { 1 } else { min_len };
        let actual_max = if max_len < actual_min {
            actual_min
        } else {
            max_len
        };
        let base = charset_len as u64;
        let mut total: u64 = 0;
        for len in actual_min..=actual_max {
            total = total.saturating_add(base.saturating_pow(len as u32));
        }
        total
    }
}

impl Iterator for BruteForceIterator {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        if self.done || self.charset.is_empty() {
            return None;
        }

        if self.exhausted {
            // 当前长度已穷尽，增加长度
            self.current_len += 1;
            if self.current_len > self.max_len {
                self.done = true;
                return None;
            }
            self.indices = vec![0; self.current_len];
            self.exhausted = false;
        }

        // 生成当前密码
        let password: String = self.indices.iter().map(|&i| self.charset[i]).collect();

        // 推进到下一个组合（从最右位开始进位）
        let charset_len = self.charset.len();
        let mut carry = true;
        for i in (0..self.indices.len()).rev() {
            if carry {
                self.indices[i] += 1;
                if self.indices[i] >= charset_len {
                    self.indices[i] = 0;
                    // carry 继续传播
                } else {
                    carry = false;
                }
            }
        }

        if carry {
            // 所有位都溢出了，当前长度穷尽
            self.exhausted = true;
        }

        Some(password)
    }
}

/// 创建暴力破解密码迭代器
pub fn generate_bruteforce_passwords(
    charset: &str,
    min_len: usize,
    max_len: usize,
) -> BruteForceIterator {
    BruteForceIterator::new(charset, min_len, max_len)
}

// ─── 恢复主循环 ──────────────────────────────────────────────────

/// 进度报告间隔（毫秒）
const PROGRESS_INTERVAL_MS: u128 = 500;

/// 恢复结果：明确区分三种终态
#[derive(Debug)]
pub enum RecoveryResult {
    /// 成功找到密码
    Found(String),
    /// 穷尽所有候选密码，未找到
    Exhausted,
    /// 用户取消
    Cancelled,
}

/// 运行密码恢复任务。
///
/// 返回值：
/// - `Ok(RecoveryResult::Found(password))` — 成功找到密码
/// - `Ok(RecoveryResult::Exhausted)` — 穷尽所有候选密码
/// - `Ok(RecoveryResult::Cancelled)` — 被用户取消
/// - `Err(msg)` — 发生错误
pub fn run_recovery(
    config: RecoveryConfig,
    file_path: String,
    app_handle: tauri::AppHandle,
    cancel_flag: Arc<AtomicBool>,
) -> Result<RecoveryResult, String> {
    let path = Path::new(&file_path);

    if !path.exists() {
        return Err(format!("文件不存在: {}", file_path));
    }

    // 打开文件并创建 ZipArchive（只做一次）
    let file = std::fs::File::open(path).map_err(|e| format!("无法打开文件: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("无法解析 ZIP 文件: {}", e))?;

    // 找到第一个加密的非目录条目的索引（只做一次）
    let encrypted_index = (0..archive.len()).find(|&i| {
        archive
            .by_index_raw(i)
            .map(|entry| entry.encrypted() && !entry.is_dir())
            .unwrap_or(false)
    });

    let index = match encrypted_index {
        Some(i) => i,
        None => return Err("该 ZIP 文件没有加密条目".to_string()),
    };

    let task_id = config.task_id.clone();
    let start_time = Instant::now();
    let mut tried: u64 = 0;
    let mut last_report_time = Instant::now();

    // 根据攻击模式确定总量和密码来源
    let (total, passwords): (u64, Box<dyn Iterator<Item = String>>) = match &config.mode {
        AttackMode::Dictionary { wordlist } => {
            let total = wordlist.len() as u64;
            let iter = wordlist.clone().into_iter();
            (total, Box::new(iter))
        }
        AttackMode::BruteForce {
            charset,
            min_length,
            max_length,
        } => {
            let total = BruteForceIterator::total_combinations(
                charset.chars().count(),
                *min_length,
                *max_length,
            );
            let iter = generate_bruteforce_passwords(charset, *min_length, *max_length);
            (total, Box::new(iter))
        }
    };

    // 发送初始进度
    let _ = app_handle.emit(
        "recovery-progress",
        RecoveryProgress {
            task_id: task_id.clone(),
            tried: 0,
            total,
            speed: 0.0,
            status: RecoveryStatus::Running,
            found_password: None,
            elapsed_seconds: 0.0,
        },
    );

    for password in passwords {
        // 检查取消标志
        if cancel_flag.load(Ordering::Relaxed) {
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                tried as f64 / elapsed
            } else {
                0.0
            };
            let _ = app_handle.emit(
                "recovery-progress",
                RecoveryProgress {
                    task_id: task_id.clone(),
                    tried,
                    total,
                    speed,
                    status: RecoveryStatus::Cancelled,
                    found_password: None,
                    elapsed_seconds: elapsed,
                },
            );
            log::info!("恢复任务已取消: {} (已尝试 {} 个密码)", task_id, tried);
            return Ok(RecoveryResult::Cancelled);
        }

        tried += 1;

        // 尝试密码（复用已打开的 archive 和已确定的 index）
        if try_password_on_archive(&mut archive, index, &password) {
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                tried as f64 / elapsed
            } else {
                0.0
            };
            let _ = app_handle.emit(
                "recovery-progress",
                RecoveryProgress {
                    task_id: task_id.clone(),
                    tried,
                    total,
                    speed,
                    status: RecoveryStatus::Found,
                    found_password: Some(password.clone()),
                    elapsed_seconds: elapsed,
                },
            );
            log::info!(
                "密码已找到: {} (尝试 {} 次, 耗时 {:.1}s)",
                task_id,
                tried,
                elapsed
            );
            return Ok(RecoveryResult::Found(password));
        }

        // 定时报告进度
        let now = Instant::now();
        if now.duration_since(last_report_time).as_millis() >= PROGRESS_INTERVAL_MS {
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                tried as f64 / elapsed
            } else {
                0.0
            };
            let _ = app_handle.emit(
                "recovery-progress",
                RecoveryProgress {
                    task_id: task_id.clone(),
                    tried,
                    total,
                    speed,
                    status: RecoveryStatus::Running,
                    found_password: None,
                    elapsed_seconds: elapsed,
                },
            );
            last_report_time = now;
        }
    }

    // 穷尽所有密码
    let elapsed = start_time.elapsed().as_secs_f64();
    let speed = if elapsed > 0.0 {
        tried as f64 / elapsed
    } else {
        0.0
    };
    let _ = app_handle.emit(
        "recovery-progress",
        RecoveryProgress {
            task_id: task_id.clone(),
            tried,
            total,
            speed,
            status: RecoveryStatus::Exhausted,
            found_password: None,
            elapsed_seconds: elapsed,
        },
    );
    log::info!(
        "密码穷尽: {} (尝试 {} 次, 耗时 {:.1}s)",
        task_id,
        tried,
        elapsed
    );

    Ok(RecoveryResult::Exhausted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures_dir() -> std::path::PathBuf {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.parent().unwrap().join("fixtures").join("zip")
    }

    // ─── BruteForceIterator ─────────────────────────────────────────

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
        // max < min → clamped to min, so we get all 3-char combos of "ab"
        let items: Vec<String> = BruteForceIterator::new("ab", 3, 1).collect();
        assert_eq!(items.len(), 8); // 2^3 = 8
        assert_eq!(items[0], "aaa");
        assert_eq!(items[7], "bbb");
    }

    // ─── total_combinations ─────────────────────────────────────────

    #[test]
    fn total_combinations_2_1_2() {
        assert_eq!(BruteForceIterator::total_combinations(2, 1, 2), 6); // 2 + 4
    }

    #[test]
    fn total_combinations_26_1_3() {
        // 26 + 676 + 17576 = 18278
        assert_eq!(BruteForceIterator::total_combinations(26, 1, 3), 18278);
    }

    #[test]
    fn total_combinations_zero_charset() {
        assert_eq!(BruteForceIterator::total_combinations(0, 1, 3), 0);
    }

    #[test]
    fn total_combinations_min_zero_treated_as_one() {
        // min 0 → 1, so same as (2, 1, 2) = 6
        assert_eq!(BruteForceIterator::total_combinations(2, 0, 2), 6);
    }

    // ─── try_password_zip ───────────────────────────────────────────

    #[test]
    fn try_password_zip_correct_on_aes() {
        let path = fixtures_dir().join("encrypted-aes.zip");
        assert!(try_password_zip(&path, "test123"));
    }

    #[test]
    fn try_password_zip_wrong_on_aes() {
        let path = fixtures_dir().join("encrypted-aes.zip");
        assert!(!try_password_zip(&path, "wrong"));
    }

    #[test]
    fn try_password_zip_correct_on_strong() {
        let path = fixtures_dir().join("encrypted-strong.zip");
        assert!(try_password_zip(&path, "Str0ng!P@ss"));
    }

    #[test]
    fn try_password_zip_on_unencrypted_returns_false() {
        let path = fixtures_dir().join("normal.zip");
        assert!(!try_password_zip(&path, "anything"));
    }

    #[test]
    fn try_password_zip_nonexistent_file_returns_false() {
        let path = fixtures_dir().join("does-not-exist.zip");
        assert!(!try_password_zip(&path, "test"));
    }

    // ─── try_password_on_archive ────────────────────────────────────

    #[test]
    fn try_password_on_archive_correct() {
        let path = fixtures_dir().join("encrypted-aes.zip");
        let file = std::fs::File::open(&path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();

        // Find the first encrypted entry index
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
        let path = fixtures_dir().join("encrypted-aes.zip");
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
}
