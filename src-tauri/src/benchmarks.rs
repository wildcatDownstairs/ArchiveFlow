// benchmarks.rs 只在测试构建中编译，用来承载手动运行的性能基线。
// 这里不用 unstable 的 cargo bench，而是用 "ignored test" 的方式提供一个
// 跨平台、可直接在 stable Rust 上运行的 benchmark 入口。

use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::Utc;

use crate::db::Database;
use crate::domain::recovery::{AttackMode, RecoveryCheckpoint, RecoveryProgress, RecoveryStatus};
use crate::domain::task::ArchiveType;
use crate::services::recovery_service::{
    try_password_on_archive, BruteForceIterator, MaskIterator,
};

// BenchmarkSample 代表一条基准记录。
// 例如 "100,000 个候选生成用了 35ms"。
#[derive(Debug)]
struct BenchmarkSample {
    name: &'static str,
    iterations: u64,
    unit: &'static str,
    elapsed: Duration,
}

impl BenchmarkSample {
    fn throughput_per_second(&self) -> f64 {
        let seconds = self.elapsed.as_secs_f64();
        if seconds == 0.0 {
            self.iterations as f64
        } else {
            self.iterations as f64 / seconds
        }
    }

    fn to_markdown_row(&self) -> String {
        format!(
            "| {} | {} {} | {:.2?} | {:.0} {}/s |",
            self.name,
            self.iterations,
            self.unit,
            self.elapsed,
            self.throughput_per_second(),
            self.unit
        )
    }
}

// fixtures 目录位于仓库根目录，所以需要从 CARGO_MANIFEST_DIR（src-tauri）
// 先回到上一级，再进入 fixtures。
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root should exist")
        .to_path_buf()
}

fn zip_fixture_path() -> PathBuf {
    workspace_root()
        .join("fixtures")
        .join("zip")
        .join("encrypted-aes.zip")
}

// 用一个通用 helper 包装“开始计时 -> 执行闭包 -> 结束计时”。
// FnOnce 表示这个闭包最多被调用一次，适合 benchmark 场景。
fn measure(
    name: &'static str,
    iterations: u64,
    unit: &'static str,
    run: impl FnOnce(),
) -> BenchmarkSample {
    let start = Instant::now();
    run();
    BenchmarkSample {
        name,
        iterations,
        unit,
        elapsed: start.elapsed(),
    }
}

fn benchmark_bruteforce_generation(limit: u64) -> BenchmarkSample {
    measure("bruteforce_generation", limit, "candidates", || {
        let produced = BruteForceIterator::new("0123456789", 6, 6)
            .take(limit as usize)
            .count() as u64;
        // black_box 告诉编译器："这个值真的会被用到"，
        // 避免优化器把整个循环当成无用代码删掉。
        black_box(produced);
    })
}

fn benchmark_mask_generation(limit: u64) -> BenchmarkSample {
    measure("mask_generation", limit, "candidates", || {
        let produced = MaskIterator::new("?d?d?d?d?d?d")
            .expect("mask benchmark should parse")
            .take(limit as usize)
            .count() as u64;
        black_box(produced);
    })
}

fn benchmark_zip_verification(iterations: u64) -> BenchmarkSample {
    measure("zip_wrong_password_check", iterations, "attempts", || {
        let path = zip_fixture_path();
        let file = std::fs::File::open(&path).expect("zip fixture should exist");
        let mut archive = zip::ZipArchive::new(file).expect("zip fixture should open");
        let encrypted_index = (0..archive.len())
            .find(|&index| {
                archive
                    .by_index_raw(index)
                    .map(|entry| entry.encrypted() && !entry.is_dir())
                    .unwrap_or(false)
            })
            .expect("fixture should contain an encrypted entry");

        for _ in 0..iterations {
            let matched =
                try_password_on_archive(&mut archive, encrypted_index, "definitely-wrong");
            black_box(matched);
        }
    })
}

fn benchmark_progress_serialization(iterations: u64) -> BenchmarkSample {
    measure("progress_serialization", iterations, "payloads", || {
        let progress = RecoveryProgress {
            task_id: "bench-task".to_string(),
            tried: 12_345,
            total: 100_000,
            speed: 456_789.0,
            status: RecoveryStatus::Running,
            found_password: None,
            elapsed_seconds: 12.5,
            worker_count: 8,
            last_checkpoint_at: Some(Utc::now()),
        };

        for _ in 0..iterations {
            let payload = serde_json::to_string(&progress).expect("progress should serialize");
            black_box(payload);
        }
    })
}

fn benchmark_checkpoint_writes(iterations: u64) -> BenchmarkSample {
    measure("checkpoint_db_write", iterations, "writes", || {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let db = Database::new(temp_dir.path().to_path_buf()).expect("temp db should initialize");
        let mode = AttackMode::Mask {
            mask: "?d?d?d?d".to_string(),
        };

        for tried in 0..iterations {
            // 每次都更新 tried 和时间戳，模拟真实恢复过程中反复刷断点。
            let checkpoint = RecoveryCheckpoint {
                task_id: "bench-task".to_string(),
                mode: mode.clone(),
                archive_type: ArchiveType::Zip,
                priority: 0,
                tried,
                total: iterations,
                updated_at: Utc::now(),
            };

            db.upsert_recovery_checkpoint(&checkpoint)
                .expect("checkpoint write should succeed");
        }
    })
}

fn run_baseline_benchmarks() -> Vec<BenchmarkSample> {
    vec![
        benchmark_bruteforce_generation(100_000),
        benchmark_mask_generation(100_000),
        benchmark_zip_verification(1_000),
        benchmark_progress_serialization(10_000),
        benchmark_checkpoint_writes(2_000),
    ]
}

#[test]
#[ignore = "manual throughput benchmark"]
fn recovery_baseline_benchmarks_print_report() {
    let samples = run_baseline_benchmarks();

    println!();
    println!("| benchmark | workload | elapsed | throughput |");
    println!("| --- | --- | --- | --- |");
    for sample in &samples {
        println!("{}", sample.to_markdown_row());
    }
    println!();

    assert_eq!(samples.len(), 5);
}
