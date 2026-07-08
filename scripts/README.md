# RMRF Scripts

## Remaining Scripts

| Script | Description |
|--------|-------------|
| `test_proxmox.sh` | Integration test on a real Proxmox host (requires SSH access to a PVE node) |
| `benchmark.sh` | Performance benchmarks for chunking, compression, and dedup |

## Removed Scripts

The following scripts have been removed — their test coverage is provided by `cargo test`:

- `test_basic.sh` → `cargo test` (integration_integrity.rs)
- `test_dedup.sh` → `cargo test` (dedup tests)
- `test_edge_cases.sh` → `cargo test` (edge case tests)
- `test_sizes.sh` → `cargo test` (size stress tests)
- `test_errors.sh` → `cargo test` (error handling tests)
- `test_vm_mock.sh` → `cargo test` (mock VM tests)
- `test_all.sh` → run `cargo test --workspace` instead

## Running Tests

```bash
# All Rust tests (unit + integration)
cargo test --workspace

# Specific test
cargo test --workspace -- test_name
```
