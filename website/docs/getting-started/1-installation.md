---
title: Installation
sidebar_position: 1
---

# Installation

To start developing with Aura, you'll need the **Aura CLI**. Follow the instructions below based on your operating system and preferred installation method.

## 🚀 Quick Install (Recommended)

You can install Aura using our official installer script. This will download the latest pre-built binary and set up your environment automatically.

### macOS & Linux
```bash
curl -fsSL https://get.aura-lang.org | sh
```

### Windows (PowerShell)
```powershell
iwr -useb https://get.aura-lang.org/install.ps1 | iex
```

## 📦 Manual Binary Download

If you prefer to install manually, you can download the latest binaries directly from our [GitHub Releases](https://github.com/aura-lang/aura/releases).

1. Download the archive for your architecture (e.g., `aura-aarch64-apple-darwin.tar.gz`).
2. Extract the archive.
3. Move the `aura` binary to a directory in your `PATH` (like `/usr/local/bin`).

## 🛠 Building from Source

Aura is built with Rust. If you have the Rust toolchain installed, you can build it from source:

1. **Clone the repository:**
   ```bash
   git clone https://github.com/aura-lang/aura-rust.git
   cd aura-rust
   ```

2. **Build the compiler:**
   ```bash
   cargo build --release
   ```

3. **Install the binary:**
   The compiled binary will be located at `./target/release/aura`. You can symlink it or move it to your path.

## ✅ Verifying Installation

Once installed, check that Aura is working correctly by running:

```bash
aura --version
```

You should see an output similar to:
`Aura version 0.1.0`

> [!TIP]
> Make sure you are using an **Apple Silicon (AArch64)** machine if you want to use the native backend, as it is our primary focus. Support for x86_64 is available via the IR/LLVM backend.
