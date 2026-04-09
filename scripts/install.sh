#!/usr/bin/env bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m' # No Color

# Configuration
REPO="auraspace/aura"
BINARY_NAME="aurac"
INSTALL_DIR="${AURA_INSTALL_DIR:-$HOME/.aura}"
BIN_DIR="$INSTALL_DIR/bin"
VERSION_FILE="$INSTALL_DIR/.version"

# Command mode
MODE="install"

# Function to print colored messages
info() {
    echo -e "${BLUE}==>${NC} $1"
}

success() {
    echo -e "${GREEN}✓${NC} $1"
}

warn() {
    echo -e "${YELLOW}!${NC} $1"
}

error() {
    echo -e "${RED}✗${NC} $1"
    exit 1
}

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Detect OS and architecture
detect_platform() {
    local os
    local arch
    
    # Detect OS
    case "$(uname -s)" in
        Darwin*)
            os="apple-darwin"
            ;;
        Linux*)
            os="unknown-linux-gnu"
            ;;
        *)
            error "Unsupported operating system: $(uname -s)"
            ;;
    esac
    
    # Detect architecture
    case "$(uname -m)" in
        x86_64)
            arch="x86_64"
            ;;
        arm64|aarch64)
            arch="aarch64"
            ;;
        *)
            error "Unsupported architecture: $(uname -m)"
            ;;
    esac
    
    echo "${arch}-${os}"
}

# Get latest release version from GitHub API
get_latest_version() {
    local api_url="https://api.github.com/repos/$REPO/releases/latest"
    
    if command_exists curl; then
        curl -s "$api_url" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
    elif command_exists wget; then
        wget -qO- "$api_url" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
    else
        error "Neither curl nor wget found. Please install one of them."
    fi
}

# Get all available versions from GitHub API
get_all_versions() {
    local api_url="https://api.github.com/repos/$REPO/releases"
    
    if command_exists curl; then
        curl -s "$api_url" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' | sort -V -r
    elif command_exists wget; then
        wget -qO- "$api_url" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' | sort -V -r
    else
        error "Neither curl nor wget found. Please install one of them."
    fi
}

# Get currently installed version
get_current_version() {
    if [ -f "$VERSION_FILE" ]; then
        cat "$VERSION_FILE"
    elif [ -f "$BIN_DIR/$BINARY_NAME" ]; then
        # Try to get version from binary
        export PATH="$BIN_DIR:$PATH"
        local version=$("$BIN_DIR/$BINARY_NAME" --version 2>&1 | grep -oE 'v?[0-9]+\.[0-9]+\.[0-9]+' || echo "")
        if [ -n "$version" ]; then
            # Add 'v' prefix if not present
            [[ "$version" != v* ]] && version="v$version"
            echo "$version"
        else
            echo "unknown"
        fi
    else
        echo "not_installed"
    fi
}

# Save installed version to file
save_version() {
    local version=$1
    mkdir -p "$INSTALL_DIR"
    echo "$version" > "$VERSION_FILE"
}

# List all available versions
list_versions() {
    info "Fetching available versions..."
    local versions=$(get_all_versions)
    
    if [ -z "$versions" ]; then
        error "Could not fetch versions from GitHub"
    fi
    
    local current=$(get_current_version)
    local latest=$(get_latest_version)
    
    echo ""
    echo "Available versions:"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    
    local count=1
    while IFS= read -r version; do
        local marker=""
        
        if [ "$version" = "$current" ]; then
            marker="${GREEN}(installed)${NC}"
        fi
        
        if [ "$version" = "$latest" ]; then
            marker="${marker} ${CYAN}(latest)${NC}"
        fi
        
        printf "%3d) ${MAGENTA}%s${NC} %b\n" "$count" "$version" "$marker"
        count=$((count + 1))
    done <<< "$versions"
    
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
}

# Interactive version selector
select_version_interactive() {
    info "Fetching available versions..."
    local versions=$(get_all_versions)
    
    if [ -z "$versions" ]; then
        error "Could not fetch versions from GitHub"
    fi
    
    # Convert to array
    local versions_array=()
    while IFS= read -r version; do
        versions_array+=("$version")
    done <<< "$versions"
    
    local current=$(get_current_version)
    local latest=$(get_latest_version)
    
    echo ""
    echo "Select a version to install:"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    
    for i in "${!versions_array[@]}"; do
        local version="${versions_array[$i]}"
        local marker=""
        
        if [ "$version" = "$current" ]; then
            marker="${GREEN}(installed)${NC}"
        fi
        
        if [ "$version" = "$latest" ]; then
            marker="${marker} ${CYAN}(latest)${NC}"
        fi
        
        printf "%3d) ${MAGENTA}%s${NC} %b\n" "$((i + 1))" "$version" "$marker"
    done
    
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    
    # Read user selection
    local selection=""
    while true; do
        read -p "Enter selection (1-${#versions_array[@]}) or 'q' to quit: " selection
        
        if [ "$selection" = "q" ] || [ "$selection" = "Q" ]; then
            info "Installation cancelled"
            exit 0
        fi
        
        if [[ "$selection" =~ ^[0-9]+$ ]] && [ "$selection" -ge 1 ] && [ "$selection" -le "${#versions_array[@]}" ]; then
            echo "${versions_array[$((selection - 1))]}"
            return 0
        else
            warn "Invalid selection. Please enter a number between 1 and ${#versions_array[@]}"
        fi
    done
}

# Check if update is available
check_for_update() {
    local current=$(get_current_version)
    local latest=$(get_latest_version)
    
    if [ "$current" = "not_installed" ]; then
        return 1
    fi
    
    if [ "$current" = "unknown" ]; then
        warn "Cannot determine current version"
        return 1
    fi
    
    if [ "$current" != "$latest" ]; then
        return 0
    else
        return 1
    fi
}

# Download and extract binary
download_and_install() {
    local version=$1
    local platform=$2
    local archive_name="${BINARY_NAME}-${platform}.tar.gz"
    local download_url="https://github.com/$REPO/releases/download/${version}/${archive_name}"
    local temp_dir=$(mktemp -d)
    
    info "Downloading Aura ${version} for ${platform}..."
    
    # Download
    if command_exists curl; then
        if ! curl -fSL "$download_url" -o "$temp_dir/$archive_name"; then
            error "Failed to download Aura from $download_url"
        fi
    elif command_exists wget; then
        if ! wget -q "$download_url" -O "$temp_dir/$archive_name"; then
            error "Failed to download Aura from $download_url"
        fi
    fi
    
    success "Downloaded successfully"
    
    # Extract
    info "Extracting..."
    tar -xzf "$temp_dir/$archive_name" -C "$temp_dir"
    
    # Create installation directory
    mkdir -p "$BIN_DIR"
    
    # Move binary
    if [ -f "$temp_dir/$BINARY_NAME" ]; then
        mv "$temp_dir/$BINARY_NAME" "$BIN_DIR/$BINARY_NAME"
        chmod +x "$BIN_DIR/$BINARY_NAME"
        success "Installed to $BIN_DIR/$BINARY_NAME"
        
        # Save version
        save_version "$version"
    else
        error "Binary not found in archive"
    fi
    
    # Cleanup
    rm -rf "$temp_dir"
}

# Add to PATH
setup_path() {
    local shell_config=""
    local shell_name=""
    
    # Detect shell and config file
    # Use $SHELL to detect the user's actual shell, not the script's shell
    case "$SHELL" in
        */zsh)
            shell_name="zsh"
            shell_config="$HOME/.zshrc"
            ;;
        */bash)
            shell_name="bash"
            shell_config="$HOME/.bashrc"
            [ -f "$HOME/.bash_profile" ] && shell_config="$HOME/.bash_profile"
            ;;
        */fish)
            shell_name="fish"
            shell_config="$HOME/.config/fish/config.fish"
            ;;
        *)
            # Fallback to checking version variables
            if [ -n "$ZSH_VERSION" ]; then
                shell_name="zsh"
                shell_config="$HOME/.zshrc"
            elif [ -n "$BASH_VERSION" ]; then
                shell_name="bash"
                shell_config="$HOME/.bashrc"
                [ -f "$HOME/.bash_profile" ] && shell_config="$HOME/.bash_profile"
            elif [ -f "$HOME/.profile" ]; then
                shell_name="sh"
                shell_config="$HOME/.profile"
            fi
            ;;
    esac
    
    # Check if already in PATH
    if echo "$PATH" | grep -q "$BIN_DIR"; then
        success "Aura is already in your PATH"
        return
    fi
    
    # Add to PATH in shell config
    if [ -n "$shell_config" ]; then
        info "Adding Aura to PATH in $shell_config..."
        
        if ! grep -q "AURA_INSTALL_DIR" "$shell_config" 2>/dev/null; then
            echo "" >> "$shell_config"
            echo "# Aura" >> "$shell_config"
            echo "export AURA_INSTALL_DIR=\"$INSTALL_DIR\"" >> "$shell_config"
            echo "export PATH=\"\$AURA_INSTALL_DIR/bin:\$PATH\"" >> "$shell_config"
            success "Added Aura to PATH"
        else
            success "Aura PATH configuration already exists"
        fi
    else
        warn "Could not detect shell configuration file"
        warn "Please manually add the following to your shell configuration:"
        echo ""
        echo "  export AURA_INSTALL_DIR=\"$INSTALL_DIR\""
        echo "  export PATH=\"\$AURA_INSTALL_DIR/bin:\$PATH\""
        echo ""
    fi
}

# Verify installation
verify_installation() {
    # Temporarily add to PATH for verification
    export PATH="$BIN_DIR:$PATH"
    
    if ! command_exists "$BINARY_NAME"; then
        # Detect shell config for source instruction
        local shell_config=""
        case "$SHELL" in
            */zsh) shell_config="$HOME/.zshrc" ;;
            */bash) 
                shell_config="$HOME/.bashrc"
                [ -f "$HOME/.bash_profile" ] && shell_config="$HOME/.bash_profile"
                ;;
            */fish) shell_config="$HOME/.config/fish/config.fish" ;;
        esac
        
        echo ""
        error "Installation verification failed. $BINARY_NAME not found in PATH"
        
        if [ -n "$shell_config" ]; then
            echo ""
            echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
            warn "⚠️  To use aurac in this terminal, run:"
            echo ""
            echo "    \033[0;36msource $shell_config\033[0m"
            echo ""
            info "Or restart your terminal"
            echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
            echo ""
        fi
        exit 1
    fi
    
    local installed_version=$("$BINARY_NAME" --version 2>&1 || echo "unknown")
    success "Aura installed successfully!"
    info "Version: $installed_version"
}

# Update Aura to the latest version
update_aura() {
    info "Checking for updates..."
    
    local current=$(get_current_version)
    local latest=$(get_latest_version)
    
    if [ "$current" = "not_installed" ]; then
        error "Aura is not installed. Use install mode instead."
    fi
    
    if [ -z "$latest" ]; then
        error "Could not determine latest version"
    fi
    
    echo ""
    info "Current version: ${MAGENTA}$current${NC}"
    info "Latest version:  ${CYAN}$latest${NC}"
    echo ""
    
    if [ "$current" = "$latest" ]; then
        success "You already have the latest version!"
        exit 0
    fi
    
    info "Update available: $current → $latest"
    
    # Confirm update
    read -p "Do you want to update? (Y/n): " -n 1 -r
    echo
    
    if [[ ! $REPLY =~ ^[Yy]$ ]] && [[ -n $REPLY ]]; then
        info "Update cancelled"
        exit 0
    fi
    
    # Detect platform and install
    local platform=$(detect_platform)
    
    if [[ "$platform" != "aarch64-apple-darwin" ]]; then
        warn "Pre-built binaries are currently only available for aarch64-apple-darwin"
        warn "Your platform: $platform"
        echo ""
        error "Please build from source: https://github.com/$REPO"
    fi
    
    download_and_install "$latest" "$platform"
    
    echo ""
    success "Updated successfully!"
    info "Old version: $current"
    info "New version: $latest"
    echo ""
}

# Print usage information
print_usage() {
    cat << EOF
Aura Installation Script

USAGE:
    curl -sSL https://raw.githubusercontent.com/$REPO/main/install.sh | bash -s -- [OPTIONS]

OPTIONS:
    --help, -h              Show this help message
    --update, -u            Update to the latest version
    --list, -l              List all available versions
    --select, -s            Interactively select a version to install
    --version <VERSION>     Install a specific version (e.g., v0.1.0)
    --current               Show currently installed version

ENVIRONMENT VARIABLES:
    AURA_VERSION            Install specific version
    AURA_INSTALL_DIR        Custom installation directory (default: ~/.aura)

EXAMPLES:
    # Install latest version
    curl -sSL https://raw.githubusercontent.com/$REPO/main/install.sh | bash

    # Update to latest version
    curl -sSL https://raw.githubusercontent.com/$REPO/main/install.sh | bash -s -- --update

    # List all versions
    curl -sSL https://raw.githubusercontent.com/$REPO/main/install.sh | bash -s -- --list

    # Interactively select version
    curl -sSL https://raw.githubusercontent.com/$REPO/main/install.sh | bash -s -- --select

    # Install specific version
    curl -sSL https://raw.githubusercontent.com/$REPO/main/install.sh | bash -s -- --version v0.1.0

    # Install with environment variable
    curl -sSL https://raw.githubusercontent.com/$REPO/main/install.sh | AURA_VERSION=v0.1.0 bash

    # Custom install directory
    curl -sSL https://raw.githubusercontent.com/$REPO/main/install.sh | AURA_INSTALL_DIR=/usr/local bash

    # Show current version
    curl -sSL https://raw.githubusercontent.com/$REPO/main/install.sh | bash -s -- --current

DOWNLOAD ONLY:
    # Download script for offline use
    curl -sSL https://raw.githubusercontent.com/$REPO/main/install.sh -o install-aura.sh
    chmod +x install-aura.sh
    
    # Then use it locally
    ./install-aura.sh --help
    ./install-aura.sh --update
    ./install-aura.sh --select

EOF
}

# Parse command line arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --help|-h)
                print_usage
                exit 0
                ;;
            --update|-u)
                MODE="update"
                shift
                ;;
            --list|-l)
                MODE="list"
                shift
                ;;
            --select|-s)
                MODE="select"
                shift
                ;;
            --version)
                if [ -n "$2" ] && [ "${2:0:1}" != "-" ]; then
                    AURA_VERSION="$2"
                    shift 2
                else
                    error "Option --version requires a version argument"
                fi
                ;;
            --current)
                MODE="current"
                shift
                ;;
            *)
                error "Unknown option: $1\n\nUse --help for usage information"
                ;;
        esac
    done
}

# Main installation flow
main() {
    # Parse command line arguments
    parse_args "$@"
    
    echo ""
    echo "╔═══════════════════════════════════╗"
    echo "║   Aura Programming Language      ║"
    echo "║   Installation Script            ║"
    echo "╚═══════════════════════════════════╝"
    echo ""
    
    # Handle different modes
    case "$MODE" in
        list)
            list_versions
            exit 0
            ;;
        current)
            local current=$(get_current_version)
            if [ "$current" = "not_installed" ]; then
                warn "Aura is not installed"
                info "Install with: curl -sSL https://raw.githubusercontent.com/$REPO/main/install.sh | bash"
            else
                info "Installed version: ${MAGENTA}$current${NC}"
            fi
            exit 0
            ;;
        update)
            update_aura
            exit 0
            ;;
        select)
            local version=$(select_version_interactive)
            if [ -z "$version" ]; then
                error "No version selected"
            fi
            info "Selected version: $version"
            ;;
        install)
            # Default install mode
            ;;
        *)
            error "Unknown mode: $MODE"
            ;;
    esac
    
    # Detect platform
    local platform=$(detect_platform)
    info "Detected platform: $platform"
    
    # Get version to install
    local version="${AURA_VERSION:-}"
    if [ -z "$version" ]; then
        info "Fetching latest version..."
        version=$(get_latest_version)
        
        if [ -z "$version" ]; then
            error "Could not determine latest version"
        fi
        
        info "Latest version: $version"
    else
        info "Installing version: $version"
    fi
    
    # Check if already installed
    local current=$(get_current_version)
    if [ "$current" = "$version" ] && [ "$MODE" != "select" ]; then
        echo ""
        success "Aura $version is already installed!"
        info "Use --update to update to the latest version"
        info "Use --select to pick a different version"
        echo ""
        
        # Still setup PATH even if already installed
        setup_path
        
        # Verify installation
        verify_installation
        
        echo ""
        info "Get started with: aurac --help"
        info "Documentation: https://github.com/$REPO"
        echo ""
        exit 0
    fi
    
    # Check if platform binary is available
    # Note: Currently only aarch64-apple-darwin is built
    if [[ "$platform" != "aarch64-apple-darwin" ]]; then
        warn "Pre-built binaries are currently only available for aarch64-apple-darwin"
        warn "Your platform: $platform"
        echo ""
        error "Please build from source: https://github.com/$REPO"
    fi
    
    # Download and install
    download_and_install "$version" "$platform"
    
    # Setup PATH
    setup_path
    
    # Verify installation
    verify_installation
    
    echo ""
    echo "╔═══════════════════════════════════╗"
    echo "║   Installation Complete! 🎉      ║"
    echo "╚═══════════════════════════════════╝"
    echo ""
    info "Get started with: aurac --help"
    info "Documentation: https://github.com/$REPO"
    
    # Show update tip if not on latest
    if check_for_update 2>/dev/null; then
        local latest=$(get_latest_version)
        echo ""
        info "Tip: Update to the latest version ($latest) with:"
        echo "  curl -sSL https://raw.githubusercontent.com/$REPO/main/install.sh | bash -s -- --update"
    fi
    
    echo ""
}

# Run main function
main "$@"
