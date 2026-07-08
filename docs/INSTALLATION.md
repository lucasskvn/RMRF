# Installation Guide

Complete setup instructions for RMRF on Linux systems.

## Prerequisites

### Required
- **Rust**: 1.96.0 or later ([install rustup](https://rustup.rs/))
- **Linux**: Ubuntu 20.04+, Debian 11+, Fedora, or compatible

Verify Rust installation:
```bash
rustc --version
cargo --version
```

### Optional (for VM backup)

#### Proxmox VE
```bash
apt-get install proxmox-backup-client
qm --version      # QEMU VM management
pct --version     # LXC container management
pvesm --version   # Storage management
```

#### Libvirt (KVM/QEMU)
```bash
apt-get install libvirt-daemon-system virtinst
virsh --version
```

## Building from Source

### 1. Get the source
```bash
cd RMRF
```

### 2. Build release binary
```bash
# Development build (faster compile, slower runtime)
cargo build

# Release build (optimized, recommended)
cargo build --release
```

Binary locations:
- Development: `./target/debug/rmrf`
- Release: `./target/release/rmrf`

### 3. Verify installation
```bash
./target/release/rmrf --help
```

## Installation Options

### Option 1: System-wide
```bash
sudo cp target/release/rmrf /usr/local/bin/
rmrf --help
```

### Option 2: Local user
```bash
mkdir -p ~/.local/bin
cp target/release/rmrf ~/.local/bin/

# Add to ~/.bashrc or ~/.zshrc:
export PATH="$HOME/.local/bin:$PATH"

source ~/.bashrc
rmrf --help
```

### Option 3: Project directory
```bash
./target/release/rmrf --help
alias rmrf='~/path/to/RMRF/target/release/rmrf'
```

## Verification

### Test 1: Help text
```bash
rmrf --help
```

### Test 2: Quick backup
```bash
echo "test data" > test.txt
rmrf backup test.txt -s ./test-store
rmrf list -s ./test-store
```

### Test 3: Restore
```bash
manifest=$(find ./test-store/manifests -name "*.bincode" | head -1)
rmrf restore "$manifest" -s ./test-store -o restored.txt
diff test.txt restored.txt && echo "✅ Success!"
```

## Troubleshooting

### Build fails: "cannot find -lzstd"
```bash
apt-get install libzstd-dev
cargo build --release
```

### Rust version too old
```bash
rustup update
rustc --version  # should be 1.96.0+
```

### Build cache issues
```bash
cargo clean
cargo build --release
```

### VM backup: tools not found
```bash
# For Proxmox
apt-get install proxmox-backup-client

# For Libvirt
apt-get install libvirt-daemon-system
```

## Platform-Specific

### Ubuntu/Debian
```bash
apt-get install build-essential cargo rustc libzstd-dev
cargo build --release
sudo cp target/release/rmrf /usr/local/bin/
```

### Fedora/RHEL
```bash
dnf install gcc cargo rust libzstd-devel
cargo build --release
sudo cp target/release/rmrf /usr/local/bin/
```

### Alpine
```bash
apk add build-base cargo rust zstd-dev
cargo build --release
cp target/release/rmrf /usr/local/bin/
```

## Storage Requirements

### Minimum
- Source code: ~50 MB
- Build artifacts: ~300 MB
- Binary: ~3 MB

### Recommended
- Backup store: 2x-10x original data size (depends on compression/dedup)
- Example: 1 GB of text → ~100-200 MB backed up

## Configuration (Optional)

Create `~/.rmrf.toml` for automation:

```toml
store = "~/.rmrf-store"
compression_level = 10
cdc_min = 4096
cdc_avg = 16384
cdc_max = 65536

[[jobs]]
name = "daily-docs"
path = "/home/user/Documents"
schedule = "0 1 * * *"
retain = 30

[[jobs]]
name = "weekly-vm"
vm = "100"
schedule = "0 0 * * 0"
retain = 90
```

## Next Steps

1. **First backup**: See [USAGE.md](USAGE.md) for examples
2. **VM setup**: If backing up VMs, configure Proxmox/Libvirt access
3. **Automation**: Set up cron jobs or daemon mode
4. **Monitoring**: Use `list` and `verify` commands

## Help

```bash
rmrf --help
rmrf backup --help
rmrf restore --help
```

See [USAGE.md](USAGE.md) for complete guide.
