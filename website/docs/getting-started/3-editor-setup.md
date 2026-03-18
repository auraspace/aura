---
title: Editor Setup
sidebar_position: 3
---

# Editor Setup

To get a first-class developer experience with Aura, we recommend using **Visual Studio Code** with our official extension.

## 🧩 VS Code Extension

The Aura extension provides:

- **Syntax Highlighting**: Beautiful colors for Aura keywords and types.
- **Auto-completion**: Intelligent suggestions based on our built-in LSP.
- **Formatting**: Keep your code clean and consistent with a single shortcut (`Shift + Alt + F`).
- **Real-time Diagnostics**: Find and fix errors as you type with inline squiggles.

### Installation

1. Open **VS Code**.
2. Go to the **Extensions** view (`Ctrl + Shift + X`).
3. Search for **"Aura Language Support"**.
4. Click **Install**.

## 🛠 The Aura LSP

If you use other editors (like Neovim, Emacs, or Helix), you can still get support! The Aura compiler comes with a built-in **Language Server Protocol (LSP)** server.

### Config for other editors

Point your LSP client to the `aura` binary with the `lsp` argument. For example, in Neovim:

```lua
-- Example Neovim setup
require'lspconfig'.aura.setup{
  cmd = { "aura", "lsp" },
  filetypes = { "aura" },
  root_dir = function() return vim.loop.cwd() end,
}
```

## 🧹 Code Formatting

If you prefer to format files via the CLI, use the `fmt` command:

```bash
# Format a specific file
aura fmt main.aura

# Format all files in a directory
aura fmt .
```
