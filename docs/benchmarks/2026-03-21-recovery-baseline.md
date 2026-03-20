# Recovery Benchmark Baseline

## Scope

This baseline intentionally measures hot paths that can be run repeatedly without touching the live Tauri UI thread:

- brute-force candidate generation
- mask candidate generation
- ZIP wrong-password verification
- recovery progress payload serialization
- recovery checkpoint database writes

These numbers are not meant to replace end-to-end testing. They exist to answer a narrower question: after future optimizations, did the hot path actually get faster?

## How to Run

From the repository root:

```bash
npm run bench:recovery
```

That command runs the ignored Rust benchmark test in release mode and prints a Markdown table.

## Profiling Workflow

1. Run `npm run bench:recovery` and keep the output as the throughput baseline.
2. If one benchmark regresses, profile only that hotspot instead of the whole app.
3. For Rust CPU hotspots, prefer sampling profilers first:
   - Windows: Visual Studio Profiler or Windows Performance Recorder / Analyzer
   - macOS: Instruments Time Profiler
   - Linux/macOS: `cargo flamegraph` if available
4. Re-run the same benchmark after the change and compare throughput, not just wall-clock impressions.

## Current Baseline

Run date: 2026-03-21

| benchmark | workload | elapsed | throughput |
| --- | --- | --- | --- |
| brute-force generation | 100,000 candidates | 3.37 ms | 29,711,501 candidates/s |
| mask generation | 100,000 candidates | 3.54 ms | 28,262,160 candidates/s |
| ZIP wrong-password verification | 1,000 attempts | 284.25 ms | 3,518 attempts/s |
| progress serialization | 10,000 payloads | 2.28 ms | 4,390,779 payloads/s |
| checkpoint DB write | 2,000 writes | 5.43 s | 368 writes/s |

Primary observation:

- candidate generation is not the current bottleneck
- ZIP verification is materially slower than candidate generation, but still far ahead of checkpoint persistence
- checkpoint persistence is the slowest measured hotspot and should stay batched/throttled during future engine work

The current baseline should still be regenerated on the target machine before doing deep optimization work, especially on:

- Intel hybrid CPUs such as i9-13900K
- Apple Silicon machines where performance and efficiency cores share work differently
