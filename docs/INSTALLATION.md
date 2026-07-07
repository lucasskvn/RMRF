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
# Install Proxmox SDK
apt-get install proxmox-backup-client

# Verify tools available
qm --version      # QEMU VM management
pct --version     # LXC container management
pvesm --version   # Storage management
```

#### Libvirt (KVM/QEMU)
```bash
# Install Libvirt
apt-get install libvirt-daemon-system virtinst

# Verify virsh available
virsh --version
```

#### KVM
```bash
# Install KVM tools
apt-get install qemu-kvm libvirt-daemon

# Verify KVM support
kvm-ok
```

## Building from Source

### 1. Clone or download the repository
```bash
cd RMRF
```

### 2. Build the project
```bash
# Development build (debug, slower)
cargo build

# Release build (optimized, recommended)
cargo build --release
```

The binary will be located at:
- Development: `./target/debug/rmrf`
- Release: `./target/release/rmrf`

### 3. Verify installation
```bash
./target/release/rmrf --help
```

## Installation Options

### Option 1: System-wide Installation
```bash
sudo cp target/release/rmrf /usr/local/bin/
rmrf --help
```

### Option 2: Local User Installation
```bash
mkdir -p ~/.local/bin
cp target/release/rmrf ~/.local/bin/
# Add to ~/.bashrc: export PATH="$HOME/.local/bin:$PATH"
source ~/.bashrc
rmrf --help
```

### Option 3: Project Directory Usage
```bash
./target/release/rmrf --help
alias rmrf='~/path/to/RMRF/target/release/rmrf'
```

## Verifying Installation

### Test 1: Create test backup
```bash
echo "Hello RMRF" > test.txt
rmrf backup test.txt -s ./test-store
rmrf list -s ./test-store
```

### Test 2: Restore
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

### VM backup tools missing
```bash
# For Proxmox
apt-get install proxmox-backup-client

# For Libvirt
apt-get install libvirt-daemon-system
```

## Platform-Specific Installation

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

## Next Steps

1. Check [USAGE.md](USAGE.md) for first backup
2. Configure VM backup if needed
3. Set up cron jobs for scheduled backups

---

See [USAGE.md](USAGE.md) for complete examples.
