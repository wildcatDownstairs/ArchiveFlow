# Capability Enhancement & Engineering Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add result export, strategy expansion, settings enhancement, checkpoint/resume, benchmarks, task scheduler, UI observability, and test coverage.

**Architecture:** Feature-by-feature implementation. Each feature touches Rust backend (commands/services/domain/db) + React frontend (api/types/i18n/pages/components). DB changes via migration v5.

**Tech Stack:** Tauri 2, Rust, React 19, TypeScript 5.9, SQLite (rusqlite), Zustand, react-i18next, lucide-react

---

## Phase 1: Capability Enhancement

### Task 1: Recovery Result Export

**Files:**
- Create: `src-tauri/src/commands/export_commands.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs` (register commands)
- Modify: `src/services/api.ts` (add exportTaskResult, exportAllTasks)
- Modify: `src/types/index.ts` (ExportFormat type)
- Modify: `src/i18n/index.ts` (export-related keys)
- Modify: `src/pages/TaskDetailPage.tsx` (export button)
- Modify: `src/pages/TaskPage.tsx` (batch export button)

**Design:**
- Backend: `export_tasks` command takes `task_ids: Vec<String>` and `format: "csv"|"json"`
- Returns serialized string content
- Frontend uses Tauri `save` dialog + `writeTextFile` to save
- Emit `ResultExported` audit event (already defined in AuditEventType)

### Task 2: Recovery Strategy Expansion — Mask Attack

**Files:**
- Modify: `src-tauri/src/domain/recovery.rs` (add `MaskAttack` variant)
- Modify: `src-tauri/src/services/recovery_service.rs` (mask iterator + shard support)
- Modify: `src-tauri/src/commands/recovery_commands.rs` (parse mask mode)
- Modify: `src/components/RecoveryPanel.tsx` (mask tab)
- Modify: `src/i18n/index.ts` (mask-related keys)

**Design:**
- `MaskAttack { pattern: String }` — pattern uses: `?l`=lowercase, `?u`=uppercase, `?d`=digit, `?s`=special, `?a`=all, literal chars
- `MaskIterator` generates all combinations for the mask pattern
- Total = product of each position's charset size
- Supports sharding via `skip_to(n)` (mixed-radix)

### Task 3: Settings Page Enhancement

**Files:**
- Modify: `src/pages/SettingsPage.tsx` (thread count, default presets)
- Modify: `src/stores/appStore.ts` (add recovery settings)
- Modify: `src/i18n/index.ts` (settings keys)
- Modify: `src/components/RecoveryPanel.tsx` (read thread count from store)
- Modify: `src-tauri/src/domain/recovery.rs` (add thread_count to RecoveryConfig)
- Modify: `src-tauri/src/services/recovery_service.rs` (use config thread count)
- Modify: `src-tauri/src/commands/recovery_commands.rs` (pass thread count)

**Design:**
- Thread count: slider 1..num_cpus, default="auto" (num_cpus-1)
- Persist to localStorage via Zustand
- Pass as optional field in recovery start, backend uses it or falls back to auto

### Task 4: Recovery Checkpoint/Resume

**Files:**
- Modify: `src-tauri/src/db/migrations.rs` (v5: add checkpoint columns)
- Modify: `src-tauri/src/db/mod.rs` (save/load checkpoint methods)
- Modify: `src-tauri/src/domain/recovery.rs` (CheckpointData struct)
- Modify: `src-tauri/src/services/recovery_service.rs` (periodic checkpoint save + resume)
- Modify: `src-tauri/src/commands/recovery_commands.rs` (resume_recovery command)
- Modify: `src/services/api.ts` (resumeRecovery)
- Modify: `src/components/RecoveryPanel.tsx` (resume button)
- Modify: `src/i18n/index.ts`

**Design:**
- DB v5: `ALTER TABLE tasks ADD COLUMN checkpoint_data TEXT` (JSON: {tried, mode, config})
- Save checkpoint every 5 seconds during recovery
- `resume_recovery` command loads checkpoint, calls `shard_passwords` with offset
- Frontend shows "Resume" button when task status is `cancelled` or `interrupted` and checkpoint_data exists

## Phase 2: Performance & Engineering

### Task 5: Benchmark & Profiling

**Files:**
- Create: `src-tauri/benches/recovery_bench.rs`
- Modify: `src-tauri/Cargo.toml` (add criterion dev-dependency + bench target)

### Task 6: Task Scheduler (Sequential Queue)

**Files:**
- Modify: `src-tauri/src/domain/recovery.rs` (TaskQueue struct)
- Create: `src-tauri/src/services/scheduler_service.rs`
- Modify: `src-tauri/src/commands/recovery_commands.rs` (queue_recovery, get_queue)
- Modify: `src/services/api.ts`
- Modify: `src/components/RecoveryPanel.tsx`

### Task 7: UI Observability Enhancement

**Files:**
- Modify: `src-tauri/src/domain/recovery.rs` (add worker_count, eta to RecoveryProgress)
- Modify: `src-tauri/src/services/recovery_service.rs` (compute ETA)
- Modify: `src/types/index.ts`
- Modify: `src/components/RecoveryPanel.tsx` (display ETA, worker count, peak speed)

### Task 8: Test Coverage

**Files:**
- Create: `src-tauri/tests/` integration tests
- Add unit tests to existing modules
- Frontend component tests
