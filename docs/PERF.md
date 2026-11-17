# Performance notes

Measured on a 12-thread Intel i5-1335U (AVX2 + AVX-VNNI), release build, WAL SQLite.

| Operation | Target | Notes |
|---|---|---|
| Search p50 (10k docs) | ≤ 6 ms | lexical + SymSpell; semantic adds ~1–3 ms |
| Search p95 | ≤ 25 ms | trigram + LIKE fallback on 1–2 char queries |
| Typo correction | ≤ 0.8 ms | SymSpell lookup is O(deletes); the MLP is ~8k MACs |
| Page render | ≤ 3 ms | server-rendered, no client framework |
| Idle RSS | ≤ 45 MB | mimalloc, prepared statements, bounded pools |
| Cold start | ≤ 350 ms | models are mmap'd, not parsed into the heap |
| Int8 GEMM | ~4–8 Gop/s | AVX2 path; VNNI dispatch ready |

## Techniques used

- Single writer task with 25 ms / 512-job batching → no `SQLITE_BUSY` storms.
- Read connection pool; WAL lets readers proceed during writes.
- Per-user spell vocabulary cached in `DashMap`, invalidated on writes.
- Runtime SIMD dispatch; scalar path kept as the differential-test oracle.
- `quiet` release profile: `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, stripped.

## Benchmarking

```sh
cargo test -p bk-search --release
cargo test -p bk-infer  --release   # differential + perf sanity
```
