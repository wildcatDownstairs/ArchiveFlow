# Parallel Recovery Engine Implementation Notes

**Date:** 2026-03-21  
**Status:** Implemented

## Implemented Shape

The production engine uses:

- `std::thread::spawn`
- upfront shard calculation
- `Arc<AtomicBool>` for cancellation
- `Arc<AtomicU64>` for tried-count aggregation
- `std::sync::mpsc::sync_channel` for success fan-in
- `num_cpus::get()` to derive the default worker count

It does **not** use Rayon scope-based data parallelism.

## Final Module Layout

The recovery service was later split so the parallel engine is easier to maintain:

- `src-tauri/src/services/recovery_service/passwords.rs`
- `src-tauri/src/services/recovery_service/generators.rs`
- `src-tauri/src/services/recovery_service/workers.rs`
- `src-tauri/src/services/recovery_service/engine.rs`

## Behavioural Notes

- worker count defaults to `max(1, num_cpus::get() - 1)`
- ZIP workers open their own archive instances because `ZipArchive` is not `Send`
- 7Z / RAR workers use stateless verification per attempt
- startup validation rejects corrupt or unsupported targets before the worker pool starts
- success, cancellation, and disconnected-channel paths all join workers so panics are not silently swallowed
- checkpoint writes are throttled by the progress loop rather than performed on every attempt

## Follow-up Work Still Worth Doing

- benchmark ZIP / 7Z / RAR separately
- profile checkpoint write frequency on high-core-count machines
- consider explicit scheduler retry/fairness policy on top of the existing recovery queue
- add automated end-to-end Tauri flows that exercise parallel recovery from the UI
