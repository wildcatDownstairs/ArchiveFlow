use std::any::Any;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sevenz_rust::{Error as SevenZError, Password, SevenZReader};
use tauri::Emitter;

use crate::domain::recovery::{AttackMode, RecoveryConfig, RecoveryProgress, RecoveryStatus};
use crate::domain::task::ArchiveType;
use crate::services::archive_service;

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

/// 尝试用给定密码打开 7z 文件。
/// 每次调用都会重新打开文件（7z 库不支持复用）。
/// 返回 true 表示密码正确，false 表示密码错误。
pub fn try_password_7z(file_path: &Path, password: &str) -> bool {
    match SevenZReader::open(file_path, Password::from(password)) {
        Ok(mut reader) => match validate_7z_payload(&mut reader) {
            Ok(true) => {
                // 验证文件确实需要密码：如果空密码也能通过验证，
                // 说明文件未加密，返回 false。
                if let Ok(mut empty_reader) = SevenZReader::open(file_path, Password::empty()) {
                    if let Ok(true) = validate_7z_payload(&mut empty_reader) {
                        return false;
                    }
                }
                true
            }
            _ => false,
        },
        Err(_) => false,
    }
}

/// 尝试用给定密码解密 RAR 文件中第一个加密条目。
/// 每次调用都会重新打开文件。
/// 返回 true 表示密码正确，false 表示密码错误。
pub fn try_password_rar(file_path: &Path, password: &str) -> bool {
    use unrar::Archive as RarArchive;

    let file_path_str = file_path.to_string_lossy().to_string();

    // 用密码打开并尝试处理（解压）第一个条目
    let open_result = RarArchive::with_password(&file_path_str, password).open_for_processing();

    let archive = match open_result {
        Ok(a) => a,
        Err(_) => return false,
    };

    let mut archive = archive;
    loop {
        let entry = match archive.read_header() {
            Ok(Some(entry)) => entry,
            Ok(None) => return false,
            Err(_) => return false,
        };

        let should_test = {
            let header = entry.entry();
            header.is_encrypted() && !header.is_directory()
        };

        if should_test {
            return entry.test().is_ok();
        }

        archive = match entry.skip() {
            Ok(next) => next,
            Err(_) => return false,
        };
    }
}

#[allow(dead_code)]
fn is_7z_password_error(error: &SevenZError) -> bool {
    matches!(
        error,
        SevenZError::PasswordRequired | SevenZError::MaybeBadPassword(_)
    )
}

fn validate_7z_payload<R: Read + Seek>(reader: &mut SevenZReader<R>) -> Result<bool, SevenZError> {
    let mut validated_any_file = false;

    reader.for_each_entries(|entry, entry_reader| {
        if entry.is_directory() || !entry.has_stream() {
            return Ok(true);
        }

        validated_any_file = true;
        std::io::copy(entry_reader, &mut std::io::sink())?;
        Ok(true)
    })?;

    Ok(validated_any_file)
}

fn validate_recovery_target(path: &Path, archive_type: &ArchiveType) -> Result<(), String> {
    let is_encrypted = match archive_type {
        ArchiveType::Zip => archive_service::inspect_zip(path)?.is_encrypted,
        ArchiveType::SevenZ => archive_service::inspect_7z(path)?.is_encrypted,
        ArchiveType::Rar => archive_service::inspect_rar(path)?.is_encrypted,
        ArchiveType::Unknown => {
            return Err("未知的归档类型，无法进行密码恢复".to_string());
        }
    };

    if is_encrypted {
        Ok(())
    } else {
        Err("当前归档没有可恢复的加密内容".to_string())
    }
}

// ─── 暴力破解密码生成器 ────────────────────────────────────────────

/// 暴力破解密码迭代器：生成指定字符集在 [min_len, max_len] 范围内的所有组合
pub struct BruteForceIterator {
    charset: Vec<char>,
    min_len: usize,
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
            min_len: actual_min,
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

    /// Advance the iterator to position `n` (0-indexed) without yielding
    /// intermediate items. After calling this, the next `next()` returns the
    /// item that would have been at global position `n`.
    /// If `n` >= total combinations the iterator is exhausted.
    pub fn skip_to(&mut self, mut n: u64) {
        if self.done || self.charset.is_empty() {
            return;
        }

        let base = self.charset.len() as u64;

        // Skip entire length groups until we find the length containing position n
        let mut len = self.min_len;
        loop {
            let count = base.saturating_pow(len as u32);
            if n < count {
                break;
            }
            n -= count;
            len += 1;
            if len > self.max_len {
                self.done = true;
                return;
            }
        }

        // Now `n` is the offset within the `len`-length group
        self.current_len = len;
        self.indices = vec![0usize; len];
        self.exhausted = false;

        // Decode n as a mixed-radix number (base = charset.len())
        let base_usize = self.charset.len();
        let mut remaining = n;
        for i in (0..len).rev() {
            self.indices[i] = (remaining as usize) % base_usize;
            remaining /= base_usize as u64;
        }
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
#[allow(dead_code)]
pub fn generate_bruteforce_passwords(
    charset: &str,
    min_len: usize,
    max_len: usize,
) -> BruteForceIterator {
    BruteForceIterator::new(charset, min_len, max_len)
}

// ─── 并行 Worker ──────────────────────────────────────────────────

/// Worker 每处理这么多密码后刷新一次 tried_counter 并检查取消标志
const BATCH_SIZE: u64 = 1_000;

/// Build a password iterator for a shard [shard_start, shard_end).
fn shard_passwords(
    mode: &AttackMode,
    shard_start: u64,
    shard_end: u64,
) -> Box<dyn Iterator<Item = String> + Send> {
    match mode {
        AttackMode::Dictionary { wordlist } => {
            let words: Vec<String> = wordlist
                .iter()
                .skip(shard_start as usize)
                .take((shard_end - shard_start) as usize)
                .cloned()
                .collect();
            Box::new(words.into_iter())
        }
        AttackMode::BruteForce {
            charset,
            min_length,
            max_length,
        } => {
            let mut iter = BruteForceIterator::new(charset, *min_length, *max_length);
            iter.skip_to(shard_start);
            Box::new(iter.take((shard_end - shard_start) as usize))
        }
    }
}

/// Core worker loop shared by all archive types.
/// Checks cancel every BATCH_SIZE iterations, updates tried_counter atomically,
/// sends found password via result_tx.
fn run_worker_inner<F>(
    passwords: impl Iterator<Item = String>,
    cancel_flag: &AtomicBool,
    tried_counter: &AtomicU64,
    result_tx: &mpsc::SyncSender<String>,
    mut try_fn: F,
) where
    F: FnMut(&str) -> bool,
{
    let mut batch_count: u64 = 0;
    for pw in passwords {
        // Check cancel at start and every BATCH_SIZE iterations
        if cancel_flag.load(Ordering::Relaxed) {
            tried_counter.fetch_add(batch_count, Ordering::Relaxed);
            return;
        }
        if batch_count >= BATCH_SIZE {
            tried_counter.fetch_add(batch_count, Ordering::Relaxed);
            batch_count = 0;
        }

        batch_count += 1;

        if try_fn(&pw) {
            // Flush remaining count before sending result
            tried_counter.fetch_add(batch_count, Ordering::Relaxed);
            let _ = result_tx.send(pw);
            return;
        }
    }
    // Flush remaining batch
    tried_counter.fetch_add(batch_count, Ordering::Relaxed);
}

fn describe_worker_panic(payload: Box<dyn Any + Send + 'static>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

fn join_worker_handles(handles: Vec<JoinHandle<()>>) -> Vec<String> {
    let mut panic_messages = Vec::new();

    for handle in handles {
        if let Err(payload) = handle.join() {
            panic_messages.push(describe_worker_panic(payload));
        }
    }

    panic_messages
}

fn format_worker_panic_error(panic_messages: &[String]) -> String {
    if panic_messages.is_empty() {
        return "恢复 worker 异常退出".to_string();
    }

    if panic_messages.len() == 1 {
        return format!("恢复 worker 异常退出: {}", panic_messages[0]);
    }

    format!(
        "{} 个恢复 worker 异常退出: {}",
        panic_messages.len(),
        panic_messages.join("; ")
    )
}

/// A single worker shard for ZIP archives.
/// Opens its own ZipArchive (ZipArchive is not Send, must be per-thread).
fn run_zip_worker_shard(
    path: PathBuf,
    mode: AttackMode,
    shard_start: u64,
    shard_end: u64,
    cancel_flag: Arc<AtomicBool>,
    tried_counter: Arc<AtomicU64>,
    result_tx: mpsc::SyncSender<String>,
) {
    // Open archive independently in this thread
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => return,
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(_) => return,
    };
    let encrypted_index = match (0..archive.len()).find(|&i| {
        archive
            .by_index_raw(i)
            .map(|entry| entry.encrypted() && !entry.is_dir())
            .unwrap_or(false)
    }) {
        Some(i) => i,
        None => return,
    };

    let passwords = shard_passwords(&mode, shard_start, shard_end);
    run_worker_inner(passwords, &cancel_flag, &tried_counter, &result_tx, |pw| {
        try_password_on_archive(&mut archive, encrypted_index, pw)
    });
}

/// A single worker shard for 7Z / RAR archives (stateless per-call).
fn run_stateless_worker_shard(
    path: PathBuf,
    archive_type: ArchiveType,
    mode: AttackMode,
    shard_start: u64,
    shard_end: u64,
    cancel_flag: Arc<AtomicBool>,
    tried_counter: Arc<AtomicU64>,
    result_tx: mpsc::SyncSender<String>,
) {
    let passwords = shard_passwords(&mode, shard_start, shard_end);
    run_worker_inner(
        passwords,
        &cancel_flag,
        &tried_counter,
        &result_tx,
        |pw| match archive_type {
            ArchiveType::SevenZ => try_password_7z(&path, pw),
            ArchiveType::Rar => try_password_rar(&path, pw),
            _ => false,
        },
    );
}

// ─── 恢复主循环（多线程并行） ────────────────────────────────────

/// 进度报告间隔（毫秒）
const PROGRESS_INTERVAL_MS: u64 = 500;

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

/// 运行密码恢复任务（多线程并行版本）。
///
/// 将候选密码空间分成 N 个分片（N = max(1, num_cpus - 1)），
/// 每个 worker 线程独立打开文件句柄并行验证。
///
/// 返回值：
/// - `Ok(RecoveryResult::Found(password))` — 成功找到密码
/// - `Ok(RecoveryResult::Exhausted)` — 穷尽所有候选密码
/// - `Ok(RecoveryResult::Cancelled)` — 被用户取消
/// - `Err(msg)` — 发生错误
pub fn run_recovery(
    config: RecoveryConfig,
    file_path: String,
    archive_type: ArchiveType,
    app_handle: tauri::AppHandle,
    cancel_flag: Arc<AtomicBool>,
) -> Result<RecoveryResult, String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("文件不存在: {}", file_path));
    }
    let path_buf = path.to_path_buf();

    validate_recovery_target(path, &archive_type)?;

    // Compute total candidates and determine shard boundaries
    let total = match &config.mode {
        AttackMode::Dictionary { wordlist } => wordlist.len() as u64,
        AttackMode::BruteForce {
            charset,
            min_length,
            max_length,
        } => BruteForceIterator::total_combinations(
            charset.chars().count(),
            *min_length,
            *max_length,
        ),
    };

    let num_workers = {
        let cpus = num_cpus::get() as u64;
        let n = std::cmp::max(1, cpus.saturating_sub(1));
        // Don't spawn more workers than there are candidates
        std::cmp::min(n, total.max(1))
    };
    let shard_size = total / num_workers;

    let shards: Vec<(u64, u64)> = (0..num_workers)
        .map(|i| {
            let start = i * shard_size;
            let end = if i == num_workers - 1 {
                total
            } else {
                start + shard_size
            };
            (start, end)
        })
        .collect();

    log::info!(
        "开始并行恢复: task={}, workers={}, total={}, archive_type={:?}",
        config.task_id,
        num_workers,
        total,
        archive_type
    );

    // Shared state
    let tried_counter = Arc::new(AtomicU64::new(0));
    let (result_tx, result_rx) = mpsc::sync_channel::<String>(1);

    // Emit initial progress
    let task_id = config.task_id.clone();
    let start_time = Instant::now();
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

    // Spawn worker threads
    let mut handles = Some(Vec::new());
    for (shard_start, shard_end) in shards {
        let path_clone = path_buf.clone();
        let mode_clone = config.mode.clone();
        let cancel_clone = Arc::clone(&cancel_flag);
        let tried_clone = Arc::clone(&tried_counter);
        let tx_clone = result_tx.clone();
        let archive_type_clone = archive_type.clone();

        let handle = std::thread::spawn(move || match archive_type_clone {
            ArchiveType::Zip => run_zip_worker_shard(
                path_clone,
                mode_clone,
                shard_start,
                shard_end,
                cancel_clone,
                tried_clone,
                tx_clone,
            ),
            ArchiveType::SevenZ | ArchiveType::Rar => run_stateless_worker_shard(
                path_clone,
                archive_type_clone,
                mode_clone,
                shard_start,
                shard_end,
                cancel_clone,
                tried_clone,
                tx_clone,
            ),
            ArchiveType::Unknown => {}
        });
        handles
            .as_mut()
            .expect("worker handles should be available before joining")
            .push(handle);
    }
    // Drop original sender so channel closes when all workers finish
    drop(result_tx);

    // Main polling loop
    let mut last_tried: u64 = 0;
    let mut last_poll_time = Instant::now();
    let poll_interval = Duration::from_millis(PROGRESS_INTERVAL_MS);

    let result = loop {
        std::thread::sleep(Duration::from_millis(50));

        let current_tried = tried_counter.load(Ordering::Relaxed);

        // Check for found password
        match result_rx.try_recv() {
            Ok(password) => {
                cancel_flag.store(true, Ordering::Relaxed);
                if let Some(worker_handles) = handles.take() {
                    let panic_messages = join_worker_handles(worker_handles);
                    for message in panic_messages {
                        log::error!("恢复 worker 在成功收敛后 panic: {}", message);
                    }
                }
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    current_tried as f64 / elapsed
                } else {
                    0.0
                };
                let _ = app_handle.emit(
                    "recovery-progress",
                    RecoveryProgress {
                        task_id: task_id.clone(),
                        tried: current_tried,
                        total,
                        speed,
                        status: RecoveryStatus::Found,
                        found_password: Some(password.clone()),
                        elapsed_seconds: elapsed,
                    },
                );
                log::info!(
                    "密码已找到: {} (尝试 {} 次, 耗时 {:.1}s, 速度 {:.0} p/s)",
                    task_id,
                    current_tried,
                    elapsed,
                    speed
                );
                break Ok(RecoveryResult::Found(password));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                if let Some(worker_handles) = handles.take() {
                    let panic_messages = join_worker_handles(worker_handles);
                    if !panic_messages.is_empty() {
                        break Err(format_worker_panic_error(&panic_messages));
                    }
                }

                // All workers finished without finding password (or cancelled)
                break if cancel_flag.load(Ordering::Relaxed) {
                    Ok(RecoveryResult::Cancelled)
                } else {
                    Ok(RecoveryResult::Exhausted)
                };
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }

        // Check external cancellation
        if cancel_flag.load(Ordering::Relaxed) {
            // Wait for workers to notice the flag and exit
            if let Some(worker_handles) = handles.take() {
                let panic_messages = join_worker_handles(worker_handles);
                for message in panic_messages {
                    log::error!("恢复 worker 在取消收敛后 panic: {}", message);
                }
            }
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                current_tried as f64 / elapsed
            } else {
                0.0
            };
            let _ = app_handle.emit(
                "recovery-progress",
                RecoveryProgress {
                    task_id: task_id.clone(),
                    tried: current_tried,
                    total,
                    speed,
                    status: RecoveryStatus::Cancelled,
                    found_password: None,
                    elapsed_seconds: elapsed,
                },
            );
            log::info!(
                "恢复任务已取消: {} (已尝试 {} 个密码)",
                task_id,
                current_tried
            );
            break Ok(RecoveryResult::Cancelled);
        }

        // Emit progress on interval
        let now = Instant::now();
        if now.duration_since(last_poll_time) >= poll_interval {
            let elapsed = start_time.elapsed().as_secs_f64();
            let delta = current_tried.saturating_sub(last_tried);
            let interval_secs = now.duration_since(last_poll_time).as_secs_f64();
            let speed = if interval_secs > 0.0 {
                delta as f64 / interval_secs
            } else {
                0.0
            };
            last_tried = current_tried;
            last_poll_time = now;

            let _ = app_handle.emit(
                "recovery-progress",
                RecoveryProgress {
                    task_id: task_id.clone(),
                    tried: current_tried,
                    total,
                    speed,
                    status: RecoveryStatus::Running,
                    found_password: None,
                    elapsed_seconds: elapsed,
                },
            );
        }
    };

    // Final status emission for exhausted/cancelled after breaking out of loop
    if let Ok(ref r) = result {
        match r {
            RecoveryResult::Exhausted => {
                let elapsed = start_time.elapsed().as_secs_f64();
                let current_tried = tried_counter.load(Ordering::Relaxed);
                let speed = if elapsed > 0.0 {
                    current_tried as f64 / elapsed
                } else {
                    0.0
                };
                let _ = app_handle.emit(
                    "recovery-progress",
                    RecoveryProgress {
                        task_id: task_id.clone(),
                        tried: current_tried,
                        total,
                        speed,
                        status: RecoveryStatus::Exhausted,
                        found_password: None,
                        elapsed_seconds: elapsed,
                    },
                );
                log::info!(
                    "密码穷尽: {} (尝试 {} 次, 耗时 {:.1}s, 速度 {:.0} p/s)",
                    task_id,
                    current_tried,
                    elapsed,
                    speed
                );
            }
            _ => {} // Found and Cancelled already emitted
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_content_encrypted_7z(password: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("hello.txt");
        let archive = dir.path().join("content-encrypted.7z");
        std::fs::write(&source, "secret payload").unwrap();
        sevenz_rust::compress_to_path_encrypted(&source, &archive, Password::from(password))
            .unwrap();
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

    // ─── skip_to ────────────────────────────────────────────────────

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

    // ─── shard_passwords ────────────────────────────────────────────

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

    // ─── total_combinations ─────────────────────────────────────────

    #[test]
    fn total_combinations_2_1_2() {
        assert_eq!(BruteForceIterator::total_combinations(2, 1, 2), 6); // 2 + 4
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

    // ─── try_password_zip ───────────────────────────────────────────

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

    // ─── try_password_on_archive ────────────────────────────────────

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

    // ─── try_password_7z ────────────────────────────────────────────

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

    // ─── try_password_rar ───────────────────────────────────────────

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

    // ─── Parallel worker integration tests ──────────────────────────

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

        run_zip_worker_shard(path, mode, 0, 5, cancel, counter.clone(), tx);

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

        run_stateless_worker_shard(path, ArchiveType::SevenZ, mode, 0, 3, cancel, counter, tx);

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

        run_stateless_worker_shard(path, ArchiveType::Rar, mode, 0, 3, cancel, counter, tx);

        let found = rx.recv().expect("should find password");
        assert_eq!(found, "unrar");
    }

    #[test]
    fn parallel_worker_respects_cancel_flag() {
        let path = zip_fixtures_dir().join("encrypted-aes.zip");
        let cancel = Arc::new(AtomicBool::new(true)); // pre-cancelled
        let counter = Arc::new(AtomicU64::new(0));
        let (tx, rx) = mpsc::sync_channel(1);

        let mode = AttackMode::Dictionary {
            wordlist: vec!["test123".to_string()],
        };

        run_zip_worker_shard(path, mode, 0, 1, cancel, counter, tx);

        // Channel should be empty — worker exited early without trying
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn parallel_multi_worker_zip_finds_password() {
        // Simulate 3 workers with password in shard 2
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
        let mode = AttackMode::Dictionary { wordlist: words };

        // Spawn 3 workers: shards [0,2), [2,4), [4,6)
        let mut handles = vec![];
        for (s, e) in [(0u64, 2u64), (2, 4), (4, 6)] {
            let p = path.clone();
            let m = mode.clone();
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
}
