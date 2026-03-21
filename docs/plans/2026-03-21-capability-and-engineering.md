# Capability Enhancement & Engineering Plan

**Date:** 2026-03-21  
**Status:** Rolling  
**Owner:** ArchiveFlow team

## Current State

The following capability items are already implemented in the codebase:

- result export (`csv` / `json`) with audit logging
- dictionary, brute-force, and mask recovery modes
- dictionary-side transforms: uppercase, capitalize, leetspeak, reverse, duplicate, year patterns, separator combinations, filename-derived seeds
- checkpoint persistence and resume after app restart
- recovery scheduler with queue, pause, resume, priority, and concurrency limit
- UI observability: ETA, worker count, latest checkpoint, recent audit events
- settings for recovery defaults, export defaults, and retention policy
- stable local benchmark entry via `npm run bench:recovery`

## Remaining High-Value Gaps

### 1. Strategy Expansion

Still worth adding:

- rule-file import for reusable transform chains
- stronger combined-dictionary generation beyond pairwise UI toggles
- common password template presets for practical recovery scenarios

### 2. Test Coverage

Current coverage is strong for Rust unit tests and frontend component tests, but still missing:

- fixture-driven end-to-end flows that exercise real Tauri commands
- regression coverage for export option combinations
- automated `tauri dev` UI flow tests

### 3. Performance Follow-up

The baseline already shows where to look next:

- checkpoint persistence remains materially slower than candidate generation
- ZIP verification is a real hotspot
- future work should compare ZIP / 7Z / RAR separately instead of treating them as one class

### 4. Scheduler Engineering

The queue exists, but deeper production behaviour is still open:

- retry policy for transient failures
- fairness / starvation avoidance when priorities diverge
- clearer task dependency hooks if future export or reporting jobs become scheduled work

## Recommended Execution Order

1. finish strategy expansion with reusable rule-file input
2. add export-combination regression tests and fixture-driven integration coverage
3. deepen benchmarks by archive type and attack mode
4. extend scheduler behaviour with explicit retry/fairness rules
5. add real Tauri end-to-end automation

## Validation Standard

Each feature point should keep the existing release gate green:

- `cargo test --manifest-path src-tauri/Cargo.toml`
- `npm run test:run`
- `npm run lint`
- `npm run build`

When benchmark-related code changes:

- `npm run bench:recovery`
