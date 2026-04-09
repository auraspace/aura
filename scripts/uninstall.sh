#!/usr/bin/env bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
INSTALL_DIR="${AURA_INSTALL_DIR:-$HOME/.aura}"

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
}

# Remove installation directory
remove_installation() {
    if [ -d "$INSTALL_DIR" ]; then
        info "Removing installation directory: $INSTALL_DIR"
        rm -rf "$INSTALL_DIR"
        success "Installation directory removed"
    else
        warn "Installation directory not found: $INSTALL_DIR"
    fi
}

# Remove PATH configuration
remove_path_config() {
    local shell_configs=(
        "$HOME/.bashrc"
        "$HOME/.bash_profile"
        "$HOME/.zshrc"
        "$HOME/.profile"
    )
    
    local found=false
    
    for config in "${shell_configs[@]}"; do
        if [ -f "$config" ] && grep -q "AURA_INSTALL_DIR" "$config" 2>/dev/null; then
            info "Removing PATH configuration from $config"
            
            # Create a backup
            cp "$config" "${config}.backup"
            
            # Remove Aura configuration lines
            sed -i.tmp '/# Aura/d' "$config"
            sed -i.tmp '/AURA_INSTALL_DIR/d' "$config"
            sed -i.tmp '/\$AURA_INSTALL_DIR\/bin/d' "$config"
            
            # Clean up empty lines
            sed -i.tmp '/^$/N;/^\n$/D' "$config"
            
            rm -f "${config}.tmp"
            
            success "Removed PATH configuration from $config"
            info "Backup created at ${config}.backup"
            found=true
        fi
    done
    
    if [ "$found" = false ]; then
        warn "No Aura PATH configuration found in shell profiles"
    fi
}

# Main uninstallation flow
main() {
    echo ""
    echo "╔═══════════════════════════════════╗"
    echo "║   Aura Uninstallation Script     ║"
    echo "╚═══════════════════════════════════╝"
    echo ""
    
    # Confirm uninstallation
    warn "This will remove Aura from your system"
    read -p "Are you sure you want to continue? (y/N): " -n 1 -r
    echo
    
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        info "Uninstallation cancelled"
        exit 0
    fi
    
    # Remove installation
    remove_installation
    
    # Remove PATH configuration
    remove_path_config
    
    echo ""
    echo "╔═══════════════════════════════════╗"
    echo "║   Uninstallation Complete!       ║"
    echo "╚═══════════════════════════════════╝"
    echo ""
    info "Aura has been removed from your system"
    warn "Please restart your terminal for PATH changes to take effect"
    echo ""
}

# Run main function
main "$@"
