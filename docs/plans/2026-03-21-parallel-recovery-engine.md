# Parallel Recovery Engine Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the single-threaded recovery loop with a `std::thread::spawn`-based parallel worker pool that saturates all CPU cores, targeting 15-20× throughput improvement on multi-core machines.

**Architecture:** Candidate space (brute-force or dictionary) is split into N equal shards where N = `max(1, num_cpus::get() - 1)`. Each worker thread opens its own file handle, runs its shard independently, and writes to shared atomic counters. The main thread polls progress every 500ms and emits Tauri events.

**Tech Stack:** Rust, `std::thread::spawn`, `std::sync::mpsc`, `AtomicU64`, `num_cpus` (cross-platform core detection), existing `zip`/`sevenz-rust`/`unrar` crates, Tauri `AppHandle::emit`.

---

### Task 1: Add or confirm dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml`

**Step 1: Add or confirm `num_cpus` in Cargo.toml**

```toml
num_cpus = "1.16"
```

Insert after the `unrar` line if it is not already present.

**Step 2: Run cargo check to confirm dependencies resolve**

```bash
cd src-tauri && cargo check 2>&1
```

Expected: no errors (warnings ok).

**Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore: 补充 num_cpus 依赖"
```

---

### Task 2: Add `skip_to` to `BruteForceIterator`

**Files:**
- Modify: `src-tauri/src/services/recovery_service.rs` (lines 159–249)

**Context:** `BruteForceIterator` currently iterates from the start. To shard the candidate space, each worker needs to start at a specific index without iterating through prior elements. `skip_to(n)` computes the combination at position `n` directly via mixed-radix arithmetic.

**Step 1: Write the failing test (add to the `#[cfg(test)]` block)**

```rust
#[test]
fn bruteforce_skip_to_matches_sequential() {
    // Items 3..6 of "abc" len 1..2:
    // index 0=a, 1=b, 2=c, 3=aa, 4=ab, 5=ac
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
    let mut iter = BruteForceIterator::new("ab", 1, 2).skip_to_mut(999);
    assert!(iter.next().is_none());
}
```

**Step 2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test bruteforce_skip 2>&1
```

Expected: compile error — method `skip_to` not found.

**Step 3: Implement `skip_to` on `BruteForceIterator`**

Add this method inside the `impl BruteForceIterator` block (after `total_combinations`):

```rust
/// Advance the iterator to position `n` (0-indexed) without yielding intermediate items.
/// After calling this, the next call to `next()` returns the item at position `n`.
/// If `n` >= total combinations, the iterator is exhausted.
pub fn skip_to(&mut self, mut n: u64) {
    if self.done || self.charset.is_empty() {
        return;
    }
    let base = self.charset.len() as u64;

    // Skip entire length groups until we find the length that contains position n
    let mut len = self.current_len;
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

    // Now n is the offset within the `len`-length group
    self.current_len = len;
    self.indices = vec![0usize; len];
    self.exhausted = false;

    // Decode n as a mixed-radix number (base = charset.len())
    let base = self.charset.len();
    for i in (0..len).rev() {
        self.indices[i] = (n as usize) % base;
        n /= base as u64;
    }
}
```

Note: the test above references `skip_to_mut` as a chainable version. Add a convenience wrapper:

```rust
/// Chainable version of skip_to for use in test expressions.
#[cfg(test)]
pub fn skip_to_mut(mut self, n: u64) -> Self {
    self.skip_to(n);
    self
}
```

**Step 4: Run tests to verify they pass**

```bash
cd src-tauri && cargo test bruteforce_skip 2>&1
```

Expected: 3 tests pass.

**Step 5: Run all existing tests to ensure no regression**

```bash
cd src-tauri && cargo test 2>&1
```

Expected: all 77+ tests pass.

**Step 6: Commit**

```bash
git add src-tauri/src/services/recovery_service.rs
git commit -m "feat: add skip_to() to BruteForceIterator for sharded parallel execution"
```

---

### Task 3: Implement the parallel `run_recovery` function

**Files:**
- Modify: `src-tauri/src/services/recovery_service.rs` (function `run_recovery`, lines 274–486)

**Context:** The new implementation:
1. Detects worker count via `num_cpus`
2. Splits candidate space into N shards
3. Spawns N threads via `std::thread::spawn`, keeping explicit `JoinHandle`s for clean shutdown and panic propagation
4. Each worker opens its own archive handle
5. Main thread polls `tried_counter` every 500ms, emits progress, checks for result

**Step 1: Add imports at the top of the file**

Replace:
```rust
use std::io::{Read, Seek};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
```

With:
```rust
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
```

**Step 2: Add the worker function for ZIP**

Add this new function before `run_recovery` (after `generate_bruteforce_passwords`):

```rust
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
    run_worker_inner(passwords, cancel_flag, tried_counter, result_tx, |pw| {
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
    run_worker_inner(passwords, cancel_flag, tried_counter, result_tx, |pw| {
        match archive_type {
            ArchiveType::SevenZ => try_password_7z(&path, pw),
            ArchiveType::Rar => try_password_rar(&path, pw),
            _ => false,
        }
    });
}

/// Core worker loop shared by all archive types.
/// Checks cancel every BATCH_SIZE iterations, updates tried_counter atomically,
/// sends found password via result_tx.
const BATCH_SIZE: u64 = 1_000;

fn run_worker_inner<F>(
    passwords: impl Iterator<Item = String>,
    cancel_flag: Arc<AtomicBool>,
    tried_counter: Arc<AtomicU64>,
    result_tx: mpsc::SyncSender<String>,
    mut try_fn: F,
) where
    F: FnMut(&str) -> bool,
{
    let mut batch_count: u64 = 0;
    for pw in passwords {
        if cancel_flag.load(Ordering::Relaxed) {
            return;
        }
        batch_count += 1;
        if batch_count >= BATCH_SIZE {
            batch_count = 0;
            tried_counter.fetch_add(BATCH_SIZE, Ordering::Relaxed);
            if cancel_flag.load(Ordering::Relaxed) {
                return;
            }
        }
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

/// Build a password iterator for a shard [shard_start, shard_end).
fn shard_passwords(mode: &AttackMode, shard_start: u64, shard_end: u64) -> Box<dyn Iterator<Item = String> + Send> {
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
```

**Step 3: Replace `run_recovery` with the parallel implementation**

Replace the entire `pub fn run_recovery(...)` function (lines 274–486) with:

```rust
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

    // Validate archive can be opened before spawning workers
    match archive_type {
        ArchiveType::Zip => {
            let file = std::fs::File::open(path).map_err(|e| format!("无法打开文件: {}", e))?;
            let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("无法解析 ZIP 文件: {}", e))?;
            let has_encrypted = (0..archive.len()).any(|i| {
                archive.by_index_raw(i).map(|e| e.encrypted() && !e.is_dir()).unwrap_or(false)
            });
            if !has_encrypted {
                return Err("该 ZIP 文件没有加密条目".to_string());
            }
        }
        ArchiveType::Unknown => return Err("未知的归档类型，无法进行密码恢复".to_string()),
        _ => {}
    }

    // Compute total candidates and determine shard boundaries
    let total = match &config.mode {
        AttackMode::Dictionary { wordlist } => wordlist.len() as u64,
        AttackMode::BruteForce { charset, min_length, max_length } => {
            BruteForceIterator::total_combinations(charset.chars().count(), *min_length, *max_length)
        }
    };

    let num_workers = std::cmp::max(1, num_cpus::get().saturating_sub(1)) as u64;
    let num_workers = std::cmp::min(num_workers, total.max(1));
    let shard_size = total / num_workers;

    let shards: Vec<(u64, u64)> = (0..num_workers)
        .map(|i| {
            let start = i * shard_size;
            let end = if i == num_workers - 1 { total } else { start + shard_size };
            (start, end)
        })
        .collect();

    // Shared state
    let tried_counter = Arc::new(AtomicU64::new(0));
    let (result_tx, result_rx) = mpsc::sync_channel::<String>(1);

    // Emit initial progress
    let task_id = config.task_id.clone();
    let start_time = Instant::now();
    let _ = app_handle.emit("recovery-progress", RecoveryProgress {
        task_id: task_id.clone(),
        tried: 0,
        total,
        speed: 0.0,
        status: RecoveryStatus::Running,
        found_password: None,
        elapsed_seconds: 0.0,
    });

    // Spawn worker threads
    let mut handles = Vec::new();
    for (shard_start, shard_end) in shards {
        let path_clone = path_buf.clone();
        let mode_clone = config.mode.clone();
        let cancel_clone = Arc::clone(&cancel_flag);
        let tried_clone = Arc::clone(&tried_counter);
        let tx_clone = result_tx.clone();
        let archive_type_clone = archive_type.clone();

        let handle = std::thread::spawn(move || {
            match archive_type_clone {
                ArchiveType::Zip => run_zip_worker_shard(
                    path_clone, mode_clone, shard_start, shard_end,
                    cancel_clone, tried_clone, tx_clone,
                ),
                ArchiveType::SevenZ | ArchiveType::Rar => run_stateless_worker_shard(
                    path_clone, archive_type_clone, mode_clone, shard_start, shard_end,
                    cancel_clone, tried_clone, tx_clone,
                ),
                ArchiveType::Unknown => {}
            }
        });
        handles.push(handle);
    }
    // Drop original sender so channel closes when all workers finish
    drop(result_tx);

    // Main polling loop
    let mut last_tried: u64 = 0;
    let mut last_poll_time = Instant::now();
    let poll_interval = Duration::from_millis(500);

    loop {
        std::thread::sleep(Duration::from_millis(50));

        let now = Instant::now();
        let current_tried = tried_counter.load(Ordering::Relaxed);

        // Check for found password
        match result_rx.try_recv() {
            Ok(password) => {
                cancel_flag.store(true, Ordering::Relaxed);
                for h in handles { let _ = h.join(); }
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 { current_tried as f64 / elapsed } else { 0.0 };
                let _ = app_handle.emit("recovery-progress", RecoveryProgress {
                    task_id: task_id.clone(),
                    tried: current_tried,
                    total,
                    speed,
                    status: RecoveryStatus::Found,
                    found_password: Some(password.clone()),
                    elapsed_seconds: elapsed,
                });
                log::info!("密码已找到: {} (尝试 {} 次, 耗时 {:.1}s)", task_id, current_tried, elapsed);
                return Ok(RecoveryResult::Found(password));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                // All workers finished without finding password
                // (or cancelled externally)
                break;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }

        // Emit progress on interval
        if now.duration_since(last_poll_time) >= poll_interval {
            let elapsed = start_time.elapsed().as_secs_f64();
            let delta = current_tried.saturating_sub(last_tried);
            let interval_secs = now.duration_since(last_poll_time).as_secs_f64();
            let speed = if interval_secs > 0.0 { delta as f64 / interval_secs } else { 0.0 };
            last_tried = current_tried;
            last_poll_time = now;

            if cancel_flag.load(Ordering::Relaxed) {
                for h in handles { let _ = h.join(); }
                let _ = app_handle.emit("recovery-progress", RecoveryProgress {
                    task_id: task_id.clone(),
                    tried: current_tried,
                    total,
                    speed,
                    status: RecoveryStatus::Cancelled,
                    found_password: None,
                    elapsed_seconds: elapsed,
                });
                log::info!("恢复任务已取消: {} (已尝试 {} 个密码)", task_id, current_tried);
                return Ok(RecoveryResult::Cancelled);
            }

            let _ = app_handle.emit("recovery-progress", RecoveryProgress {
                task_id: task_id.clone(),
                tried: current_tried,
                total,
                speed,
                status: RecoveryStatus::Running,
                found_password: None,
                elapsed_seconds: elapsed,
            });
        }
    }

    // All workers finished — join threads
    for h in handles { let _ = h.join(); }

    let elapsed = start_time.elapsed().as_secs_f64();
    let current_tried = tried_counter.load(Ordering::Relaxed);
    let speed = if elapsed > 0.0 { current_tried as f64 / elapsed } else { 0.0 };

    if cancel_flag.load(Ordering::Relaxed) {
        let _ = app_handle.emit("recovery-progress", RecoveryProgress {
            task_id: task_id.clone(),
            tried: current_tried,
            total,
            speed,
            status: RecoveryStatus::Cancelled,
            found_password: None,
            elapsed_seconds: elapsed,
        });
        log::info!("恢复任务已取消: {} (已尝试 {} 个密码)", task_id, current_tried);
        return Ok(RecoveryResult::Cancelled);
    }

    let _ = app_handle.emit("recovery-progress", RecoveryProgress {
        task_id: task_id.clone(),
        tried: current_tried,
        total,
        speed,
        status: RecoveryStatus::Exhausted,
        found_password: None,
        elapsed_seconds: elapsed,
    });
    log::info!("密码穷尽: {} (尝试 {} 次, 耗时 {:.1}s)", task_id, current_tried, elapsed);
    Ok(RecoveryResult::Exhausted)
}
```

**Step 4: Add `num_cpus` use statement at the top of the file**

No explicit `use` needed — call as `num_cpus::get()` directly (crate is linked, not a module in this file).

Ensure `Cargo.toml` already has `num_cpus = "1"` (added in Task 1).

**Step 5: Ensure `AttackMode` and `ArchiveType` derive `Clone`**

Check `src-tauri/src/domain/recovery.rs` — `AttackMode` must derive `Clone` (needed for `mode_clone` in worker spawn). If not present, add `#[derive(Clone)]`.

Also check `ArchiveType` in `src-tauri/src/domain/task.rs` — must derive `Clone`.

**Step 6: Run cargo check**

```bash
cd src-tauri && cargo check 2>&1
```

Fix any compile errors before proceeding.

**Step 7: Run all tests**

```bash
cd src-tauri && cargo test 2>&1
```

Expected: all existing tests pass. The new parallel `run_recovery` does not change the public API so unit tests for `BruteForceIterator` and `try_password_*` functions still pass.

**Step 8: Commit**

```bash
git add src-tauri/src/services/recovery_service.rs
git commit -m "feat: 使用 std::thread 实现并行恢复引擎"
```

---

### Task 4: Add integration tests for parallel recovery

**Files:**
- Modify: `src-tauri/src/services/recovery_service.rs` (test module, append after existing tests)

**Context:** We need to verify that the parallel engine actually finds passwords correctly and handles cancellation, using the existing fixture files.

**Step 1: Add integration tests to the test module**

Append to the `#[cfg(test)]` block:

```rust
// ─── Parallel recovery integration tests ──────────────────────────

fn make_fake_app_handle() -> tauri::AppHandle {
    // Integration tests for run_recovery require a real AppHandle.
    // Since constructing one in unit tests is non-trivial with Tauri 2,
    // we test the worker functions directly instead.
    unreachable!("use worker-level tests instead")
}

#[test]
fn parallel_zip_worker_finds_correct_password() {
    use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}};
    use std::sync::mpsc;
    use crate::domain::recovery::AttackMode;

    let path = zip_fixtures_dir().join("encrypted-aes.zip");
    let cancel = Arc::new(AtomicBool::new(false));
    let counter = Arc::new(AtomicU64::new(0));
    let (tx, rx) = mpsc::sync_channel(1);

    // Dictionary with correct password at position 3
    let mode = AttackMode::Dictionary {
        wordlist: vec![
            "wrong1".to_string(), "wrong2".to_string(), "wrong3".to_string(),
            "test123".to_string(), "wrong4".to_string(),
        ],
    };

    run_zip_worker_shard(path, mode, 0, 5, cancel, counter.clone(), tx);

    let found = rx.recv().expect("should find password");
    assert_eq!(found, "test123");
    assert!(counter.load(Ordering::Relaxed) > 0);
}

#[test]
fn parallel_7z_worker_finds_correct_password() {
    use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}};
    use std::sync::mpsc;
    use crate::domain::recovery::AttackMode;
    use crate::domain::task::ArchiveType;

    let path = sevenz_fixtures_dir().join("encrypted.7z");
    let cancel = Arc::new(AtomicBool::new(false));
    let counter = Arc::new(AtomicU64::new(0));
    let (tx, rx) = mpsc::sync_channel(1);

    let mode = AttackMode::Dictionary {
        wordlist: vec![
            "bad1".to_string(), "bad2".to_string(), "test123".to_string(),
        ],
    };

    run_stateless_worker_shard(path, ArchiveType::SevenZ, mode, 0, 3, cancel, counter, tx);

    let found = rx.recv().expect("should find password");
    assert_eq!(found, "test123");
}

#[test]
fn parallel_rar_worker_finds_correct_password() {
    use std::sync::{Arc, atomic::{AtomicBool, AtomicU64}};
    use std::sync::mpsc;
    use crate::domain::recovery::AttackMode;
    use crate::domain::task::ArchiveType;

    let path = rar_fixtures_dir().join("encrypted.rar");
    let cancel = Arc::new(AtomicBool::new(false));
    let counter = Arc::new(AtomicU64::new(0));
    let (tx, rx) = mpsc::sync_channel(1);

    let mode = AttackMode::Dictionary {
        wordlist: vec![
            "nope".to_string(), "unrar".to_string(), "also_nope".to_string(),
        ],
    };

    run_stateless_worker_shard(path, ArchiveType::Rar, mode, 0, 3, cancel, counter, tx);

    let found = rx.recv().expect("should find password");
    assert_eq!(found, "unrar");
}

#[test]
fn parallel_worker_respects_cancel_flag() {
    use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}};
    use std::sync::mpsc;
    use crate::domain::recovery::AttackMode;

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
fn shard_passwords_bruteforce_covers_full_space() {
    use crate::domain::recovery::AttackMode;

    let mode = AttackMode::BruteForce {
        charset: "ab".to_string(),
        min_length: 1,
        max_length: 2,
    };
    // total = 2 + 4 = 6 items
    let shard_0: Vec<String> = shard_passwords(&mode, 0, 3).collect();
    let shard_1: Vec<String> = shard_passwords(&mode, 3, 6).collect();
    let full: Vec<String> = BruteForceIterator::new("ab", 1, 2).collect();

    assert_eq!([shard_0, shard_1].concat(), full);
}
```

**Step 2: Run the new tests**

```bash
cd src-tauri && cargo test parallel 2>&1
```

Expected: all 6 new tests pass.

**Step 3: Run all tests to confirm no regression**

```bash
cd src-tauri && cargo test 2>&1
```

Expected: all tests pass.

**Step 4: Commit**

```bash
git add src-tauri/src/services/recovery_service.rs
git commit -m "test: add integration tests for parallel recovery worker sharding"
```

---

### Task 5: Verify `AttackMode` and `ArchiveType` derive `Clone`

**Files:**
- Modify (if needed): `src-tauri/src/domain/recovery.rs`
- Modify (if needed): `src-tauri/src/domain/task.rs`

**Step 1: Check AttackMode**

Open `src-tauri/src/domain/recovery.rs` and look for `enum AttackMode`. If `Clone` is not in its `#[derive(...)]`, add it.

**Step 2: Check ArchiveType**

Open `src-tauri/src/domain/task.rs` and look for `enum ArchiveType`. If `Clone` is not in its `#[derive(...)]`, add it.

**Step 3: Run cargo check to confirm**

```bash
cd src-tauri && cargo check 2>&1
```

**Step 4: Commit if changes were needed**

```bash
git add src-tauri/src/domain/
git commit -m "chore: add Clone derive to AttackMode and ArchiveType for parallel worker spawning"
```

> Note: This task may already be done as part of Task 3 if the compiler catches it first. Skip if no changes needed.

---

### Task 6: Build and smoke-test the full app

**Step 1: Run the full Rust test suite**

```bash
cd src-tauri && cargo test 2>&1
```

Expected: all tests pass (77+ existing + 6 new = 83+).

**Step 2: Run a frontend type-check and lint**

```bash
cd .. && npx tsc --noEmit && npx eslint src --ext ts,tsx --max-warnings 0 2>&1
```

Expected: no errors (existing lint warnings already at zero).

**Step 3: Build the Tauri app in dev mode to confirm it compiles end-to-end**

```bash
npm run tauri dev -- --no-watch 2>&1 | head -60
```

Or just do a release build check:

```bash
cd src-tauri && cargo build --release 2>&1 | tail -20
```

Expected: `Finished release` with no errors.

**Step 4: Final commit with design doc**

```bash
git add docs/
git commit -m "docs: add parallel recovery engine design doc and implementation plan"
```

---

## Summary of Changes

| File | Change |
|---|---|
| `src-tauri/Cargo.toml` | Add `num_cpus = "1"` |
| `src-tauri/src/services/recovery_service.rs` | Add `skip_to()`, worker functions, rewrite `run_recovery()`, new tests |
| `src-tauri/src/domain/recovery.rs` | Add `Clone` to `AttackMode` (if missing) |
| `src-tauri/src/domain/task.rs` | Add `Clone` to `ArchiveType` (if missing) |
| `docs/plans/` | Design doc + this plan |

**No frontend changes required.** The Tauri event schema (`RecoveryProgress`) is unchanged.
