// 这个文件负责"并行 worker 怎么跑"。
// 它只关心：
// - 如何从一个分片里拿候选密码
// - 如何检查取消标志
// - 如何把结果通过通道回传给主线程

use std::any::Any;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

use crate::domain::recovery::AttackMode;
use crate::domain::task::ArchiveType;

use super::generators::shard_passwords;
use super::passwords::{try_password_7z, try_password_on_archive, try_password_rar};

/// Worker 每处理这么多密码后，把局部计数刷回共享计数器。
const BATCH_SIZE: u64 = 1_000;

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
            tried_counter.fetch_add(batch_count, Ordering::Relaxed);
            let _ = result_tx.send(pw);
            return;
        }
    }
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

pub(crate) fn join_worker_handles(handles: Vec<JoinHandle<()>>) -> Vec<String> {
    let mut panic_messages = Vec::new();

    for handle in handles {
        if let Err(payload) = handle.join() {
            panic_messages.push(describe_worker_panic(payload));
        }
    }

    panic_messages
}

pub(crate) fn format_worker_panic_error(panic_messages: &[String]) -> String {
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

pub(crate) fn create_result_channel<T>(
    num_workers: u64,
) -> (mpsc::SyncSender<T>, mpsc::Receiver<T>) {
    let capacity = usize::try_from(num_workers).unwrap_or(usize::MAX).max(1);
    mpsc::sync_channel(capacity)
}

pub(crate) fn run_zip_worker_shard(
    path: PathBuf,
    mode: Arc<AttackMode>,
    shard_start: u64,
    shard_end: u64,
    cancel_flag: Arc<AtomicBool>,
    tried_counter: Arc<AtomicU64>,
    result_tx: mpsc::SyncSender<String>,
) {
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

pub(crate) fn run_stateless_worker_shard(
    path: PathBuf,
    archive_type: ArchiveType,
    mode: Arc<AttackMode>,
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
