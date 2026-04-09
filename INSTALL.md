# Aura Installation Guide

This guide covers different ways to install the Aura programming language compiler (`aurac`).

## Quick Install (Recommended)

Install the latest version with a single command:

```bash
curl -sSL https://raw.githubusercontent.com/auraspace/aura/main/scripts/install.sh | bash
```

Or if you prefer `wget`:

```bash
wget -qO- https://raw.githubusercontent.com/auraspace/aura/main/scripts/install.sh | bash
```

## Update Aura

Update to the latest version:

```bash
curl -sSL https://raw.githubusercontent.com/auraspace/aura/main/scripts/install.sh | bash -s -- --update
```

Or use the short form:

```bash
curl -sSL https://raw.githubusercontent.com/auraspace/aura/main/scripts/install.sh | bash -s -- -u
```

## List Available Versions

See all available versions:

```bash
curl -sSL https://raw.githubusercontent.com/auraspace/aura/main/scripts/install.sh | bash -s -- --list
```

## Select Version Interactively

Choose a version from an interactive menu:

```bash
curl -sSL https://raw.githubusercontent.com/auraspace/aura/main/scripts/install.sh | bash -s -- --select
```

This will show you all available versions with markers for installed and latest versions.

## Install Specific Version

Using command line option:

```bash
curl -sSL https://raw.githubusercontent.com/auraspace/aura/main/scripts/install.sh | bash -s -- --version v0.1.1
```

Or using environment variable:

```bash
curl -sSL https://raw.githubusercontent.com/auraspace/aura/main/scripts/install.sh | AURA_VERSION=v0.1.1 bash
```

## Check Current Version

See which version is currently installed:

```bash
curl -sSL https://raw.githubusercontent.com/auraspace/aura/main/scripts/install.sh | bash -s -- --current
```

## Custom Installation Directory

By default, Aura is installed to `~/.aura`. To use a custom directory:

```bash
curl -sSL https://raw.githubusercontent.com/auraspace/aura/main/scripts/install.sh | AURA_INSTALL_DIR=/usr/local bash
```

## Offline Installation

Download the script for offline use:

```bash
curl -sSL https://raw.githubusercontent.com/auraspace/aura/main/scripts/install.sh -o install-aura.sh
chmod +x install-aura.sh
```

Then use it locally:

```bash
./install-aura.sh --help
./install-aura.sh --update
./install-aura.sh --select
./install-aura.sh --version v0.1.1
```

## What the Install Script Does

1. **Detects your platform** (OS and architecture)
2. **Downloads the latest release** (or specified version) from GitHub Releases
3. **Extracts and installs** the `aurac` binary to `~/.aura/bin`
4. **Updates your PATH** by adding configuration to your shell profile (`.bashrc`, `.zshrc`, etc.)
5. **Verifies the installation** to ensure everything works

## Platform Support

Currently, pre-built binaries are available for:

- **macOS Apple Silicon** (aarch64-apple-darwin) ✅

### Other Platforms

For other platforms, please [build from source](#building-from-source).

## Manual Installation

If you prefer to install manually:

1. Download the appropriate binary from [GitHub Releases](https://github.com/auraspace/aura/releases)
2. Extract the archive:
   ```bash
   tar -xzf aurac-aarch64-apple-darwin.tar.gz
   ```
3. Move the binary to a directory in your PATH:
   ```bash
   mv aurac /usr/local/bin/
   # or
   mv aurac ~/.aura/bin/
   ```
4. Make it executable:
   ```bash
   chmod +x /usr/local/bin/aurac
   ```
5. Add to PATH if needed:
   ```bash
   export PATH="$HOME/.aura/bin:$PATH"
   ```

## Building from Source

### Prerequisites

- **Rust** 1.70 or later
- **LLVM 15** (for LLVM backend)
- **Git**

### macOS

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install LLVM 15
brew install llvm@15
export LLVM_SYS_150_PREFIX=/opt/homebrew/opt/llvm@15

# Clone and build
git clone https://github.com/auraspace/aura.git
cd aura
cargo build -p aurac --release

# Install
cp target/release/aurac ~/.aura/bin/
# or
sudo cp target/release/aurac /usr/local/bin/
```

### Linux

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install LLVM 15 (Ubuntu/Debian)
sudo apt-get update
sudo apt-get install llvm-15 llvm-15-dev libclang-15-dev

# Clone and build
git clone https://github.com/auraspace/aura.git
cd aura
cargo build -p aurac --release

# Install
cp target/release/aurac ~/.aura/bin/
# or
sudo cp target/release/aurac /usr/local/bin/
```

## Verify Installation

After installation, verify that Aura is correctly installed:

```bash
aurac --version
```

You should see output like:

```
aurac 0.1.1
```

## Uninstallation

### Quick Uninstall

Use the uninstall script:

```bash
curl -sSL https://raw.githubusercontent.com/auraspace/aura/main/scripts/uninstall.sh | bash
```

### Manual Uninstall

To manually uninstall Aura:

```bash
# Remove the installation directory
rm -rf ~/.aura

# Remove PATH configuration from your shell profile
# Edit ~/.bashrc, ~/.zshrc, or ~/.profile and remove these lines:
# export AURA_INSTALL_DIR="$HOME/.aura"
# export PATH="$AURA_INSTALL_DIR/bin:$PATH"
```

## Troubleshooting

### Command not found

If you get `command not found: aurac` after installation:

1. Make sure the installation completed successfully
2. Restart your terminal or run:
   ```bash
   source ~/.bashrc  # or ~/.zshrc, ~/.profile
   ```
3. Verify the binary exists:
   ```bash
   ls -la ~/.aura/bin/aurac
   ```
4. Manually add to PATH:
   ```bash
   export PATH="$HOME/.aura/bin:$PATH"
   ```

### Check current installation

To verify if Aura is installed and which version:

```bash
curl -sSL https://raw.githubusercontent.com/auraspace/aura/main/scripts/install.sh | bash -s -- --current
```

Or if you have the binary:

```bash
aurac --version
```

### Update not working

If update fails:

1. Check your internet connection
2. Try listing versions first:
   ```bash
   curl -sSL https://raw.githubusercontent.com/auraspace/aura/main/scripts/install.sh | bash -s -- --list
   ```
3. Manually install a specific version:
   ```bash
   curl -sSL https://raw.githubusercontent.com/auraspace/aura/main/scripts/install.sh | bash -s -- --version v0.1.1
   ```

### Permission denied

If you get permission errors:

```bash
chmod +x ~/.aura/bin/aurac
```

### Platform not supported

If pre-built binaries aren't available for your platform, please [build from source](#building-from-source).

### Interactive select not working

If the interactive version selector doesn't work when piping from curl, download the script first:

```bash
curl -sSL https://raw.githubusercontent.com/auraspace/aura/main/scripts/install.sh -o install-aura.sh
chmod +x install-aura.sh
./install-aura.sh --select
```

## Getting Help

- **Documentation**: [GitHub Repository](https://github.com/auraspace/aura)
- **Issues**: [GitHub Issues](https://github.com/auraspace/aura/issues)

## Next Steps

Once installed, you can:

```bash
# See available commands
aurac --help

# Compile an Aura program
aurac your-program.aura

# Check version
aurac --version
```

Happy coding! 🚀
