---
title: Importing & Exporting
sidebar_position: 7
---

# Importing & Exporting

Aura provides a simple and efficient way to share and use code across files through its core module system.

## Exporting

By default, everything in an Aura file is private to that file. To make a variable, function, or class available to other files, use the `export` keyword.

```aura
// constants.aura
export const PI = 3.14159;
export let version = "1.0.0";

// utils.aura
export function sayHello(name: string) {
    print(`Hello, ${name}!`);
}

export class Calculator {
    public function add(a: i32, b: i32): i32 {
        return a + b;
    }
}
```

## Importing

To use an exported member from another file, use the `import` statement.

### Named Imports

You can import specific members from a file using the `{}` syntax.

```aura
// main.aura
import { sayHello, Calculator } from "./utils";
import { PI } from "./constants";

sayHello("Aura");
let calc = new Calculator();
print(calc.add(10, 20));
```

### Module Imports (Star Imports)

You can import all exported members from a file under a namespace using the `* as namespace` syntax.

```aura
// main.aura
import * as utils from "./utils";
import * as constants from "./constants";

utils.sayHello("Aura");
print(constants.PI);
```

## Best Practices

- Prefer named imports for better static analysis and modularity.
- Organize related exports into focused files (e.g., `constants.aura`, `types.aura`).
- Keep your module exports concise and avoid unnecessary public exposure.
