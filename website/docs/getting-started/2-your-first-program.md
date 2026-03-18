---
title: Your First Program
sidebar_position: 2
---

# Your First Program

Let's write and run your first Aura program.

## 1. Create a workspace
Create a new directory for your project:

```bash
mkdir hello-aura
cd hello-aura
```

## 2. Write the code
Create a file named `main.aura` and add the following code:

```aura
function main() {
    print("Hello, Aura! 🌟");
}
```

### Understanding the syntax
- `function main()`: This is the entry point of your program. Every executable Aura script needs a `main` function.
- `print(...)`: A built-in function to output text to the console.

## 3. Run it
Use the `aura run` command to compile and execute your script in one go:

```bash
aura run main.aura
```

**Output:**
```text
Hello, Aura! 🌟
```

## 🛠 Compilation vs. Execution

The Aura CLI provides two main ways to handle your code:

- **`aura run`**: Best for development. It compiles your code into a temporary binary, executes it immediately, and then cleans up.
- **`aura build`**: Best for distribution. It compiles your code into a standalone, statically-linked binary that you can run independently.

### Build a standalone binary
```bash
aura build main.aura
```

This will create an executable named `main_bin`. You can now run it without the Aura compiler:
```bash
./main_bin
```

---

> [!NOTE]
> Aura produces native code by default for **AArch64 (Apple Silicon)**. If you are on another platform, ensure you have the appropriate toolchain or use the `--ir` flag if configured.
