# Parallel Recovery Engine Design

**Date:** 2026-03-21  
**Status:** Approved  
**Author:** AI pair-programmer (Claude Sonnet 4.6)

## Problem

The current recovery engine is single-threaded. On an i9-13900K (24 logical cores), it achieves ~232,512 passwords/s. An 8-digit numeric brute-force (100M candidates) takes ~7 minutes to exhaust. The CPU is almost entirely idle.

**Goal:** Saturate all available CPU cores, targeting 15–20× throughput increase (≥3.5M p/s on i9-13900K).

## Chosen Approach: Rayon Data Parallelism (Option A)

Split the candidate space into N shards upfront (N = `max(1, num_cpus::get() - 1)`), then run each shard in a dedicated Rayon worker thread with its own file handle and archive instance.

**Why Rayon over Tokio or subprocess:**
- CPU-bound workload — Rayon's work-stealing pool is optimal; Tokio's async model adds overhead with no benefit
- No new process management complexity
- Minimal changes to existing code structure
- Cross-platform (Windows + macOS M-series) via `num_cpus`

## Architecture

### Thread count

```rust
let num_workers = std::cmp::max(1, num_cpus::get().saturating_sub(1));
```

One core reserved for the UI/Tauri thread. `num_cpus` detects logical cores on all platforms including Apple Silicon efficiency cores.

### Candidate space sharding

#### Brute-force

`BruteForceIterator` gains a `skip_to(index: u64)` method. Given total candidates T and N workers:

```
worker i → shard [i*(T/N), (i+1)*(T/N))
last worker → shard [(N-1)*(T/N), T)  // absorbs remainder
```

`skip_to` computes the starting combination directly via mixed-radix arithmetic (O(password_length)), no sequential iteration needed.

#### Dictionary

Lines are counted once; each worker seeks to its line-offset using `BufReader` + `Seek`. No full file load into memory.

### Worker contract

Each worker receives:
| Input | Type | Purpose |
|---|---|---|
| `shard` | `(u64, u64)` | Candidate range [start, end) |
| `archive_path` | `PathBuf` | Opens its own file handle |
| `cancel_flag` | `Arc<AtomicBool>` | Shared stop signal |
| `tried_counter` | `Arc<AtomicU64>` | Global progress counter |
| `result_sender` | `mpsc::SyncSender<String>` | Found password channel |

Workers check `cancel_flag` every **1,000 iterations** (not every password) to minimize atomic overhead.

ZIP: each worker calls `ZipArchive::new(File::open(&path)?)` — `ZipArchive` is not `Send`, so per-worker instances are required.  
7Z / RAR: already stateless (each attempt opens the file independently).

### Progress reporting

Main thread loop (500ms interval):
1. Read `tried_counter` atomically
2. Compute `speed = (current - last) / elapsed_ms * 1000`
3. Emit `recovery://progress` Tauri event with `RecoveryProgress`
4. `result_receiver.try_recv()` — if `Ok(password)`, set `cancel_flag`, update DB, emit success event

### Cancellation

User stops → `cancel_recovery` Tauri command → sets `cancel_flag = true` → each worker exits on next 1,000-iteration boundary.

## Data Flow

```
run_recovery()
    │
    ├── compute num_workers, shard boundaries
    │
    ├── spawn N threads (rayon::scope or std::thread)
    │   ├── worker_0: shard [0, T/N)       → tried_counter += n, result_sender.send(pw)
    │   ├── worker_1: shard [T/N, 2T/N)    → ...
    │   └── worker_N: shard [(N-1)T/N, T)  → ...
    │
    └── main loop (500ms)
            ├── emit progress event
            └── check result_receiver → found? set cancel_flag, finalize
```

## Error Handling

- `ZipArchive::new()` failure in worker → worker sends `Err` via a separate error channel; if all workers error, the task fails with `RecoveryError::ArchiveOpenFailed`
- Partial worker failure (e.g. corrupt file detected mid-run) → log warning, set `cancel_flag`, surface error to user
- Worker panic → caught by `std::thread::JoinHandle::join()` returning `Err`

## Testing Strategy

1. **Existing 76 unit tests** must continue to pass (public API unchanged)
2. **New integration tests** (in `recovery_service.rs` test module):
   - 5-worker brute-force against `fixtures/zip/encrypted_password.zip` → finds correct password
   - 5-worker brute-force against `fixtures/7z/encrypted_7z.7z` → finds correct password
   - Cancel mid-run → workers stop within 2 seconds
3. **Criterion benchmark** (optional, `benches/recovery_bench.rs`): single-thread vs multi-thread throughput on test fixtures

## Expected Performance

| Scenario | Single-thread | 23 workers (i9-13900K) |
|---|---|---|
| Throughput | ~232,512 p/s | ~3.5–4.5M p/s |
| 8-digit numeric (100M) | ~7 min | ~25–45 sec |
| Apple M3 Pro (11 cores, 10 workers) | ~200K p/s est. | ~1.8M p/s est. |

## Files to Modify

| File | Change |
|---|---|
| `Cargo.toml` | Add `rayon`, `num_cpus` dependencies |
| `src-tauri/src/services/recovery_service.rs` | Refactor `run_recovery()`, add `skip_to()` to `BruteForceIterator`, new worker functions |
| `src-tauri/src/domain/recovery.rs` | Extend `RecoveryManager` if needed (cancel_flag already `Arc<AtomicBool>`) |

No frontend changes required — progress events already use the same `RecoveryProgress` schema.
