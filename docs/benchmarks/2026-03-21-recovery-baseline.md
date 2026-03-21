# Recovery Benchmark Baseline

## Scope

This baseline intentionally measures hot paths that can be run repeatedly without touching the live Tauri UI thread:

- brute-force candidate generation
- mask candidate generation
- brute-force resume skip / shard entry
- ZIP wrong-password verification
- 7Z wrong-password verification
- RAR wrong-password verification
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
| brute-force generation | 100,000 candidates | 3.33 ms | 30,068,858 candidates/s |
| mask generation | 100,000 candidates | 3.32 ms | 30,135,005 candidates/s |
| brute-force resume skip | 100,000 candidates | 3.09 ms | 32,311,222 candidates/s |
| ZIP wrong-password verification | 1,000 attempts | 276.64 ms | 3,615 attempts/s |
| 7Z wrong-password verification | 500 attempts | 33.19 ms | 15,064 attempts/s |
| RAR wrong-password verification | 500 attempts | 5.94 s | 84 attempts/s |
| progress serialization | 10,000 payloads | 3.14 ms | 3,181,269 payloads/s |
| checkpoint DB write | 2,000 writes | 4.87 s | 411 writes/s |

Primary observation:

- candidate generation is not the current bottleneck
- brute-force shard entry (`skip_to`) is not a hotspot
- 7Z verification is materially faster than ZIP in the current wrong-password path
- RAR verification is by far the slowest measured verification hotspot
- checkpoint persistence is still slow enough that it should remain batched/throttled during future engine work

The current baseline should still be regenerated on the target machine before doing deep optimization work, especially on:

- Intel hybrid CPUs such as i9-13900K
- Apple Silicon machines where performance and efficiency cores share work differently
