---
title: 2. Interfaces
sidebar_position: 2
---

# Interfaces

Interfaces in Aura define a contract that classes must follow. They specify a set of methods (and their signatures) that an implementing class must provide.

## Interface Declaration

Use the `interface` keyword to define an interface.

```aura
interface Drawable {
    function draw(): void;
    function resize(factor: number): void;
}
```

## Implementing Interfaces

A class uses the `implements` keyword to indicate that it adheres to one or more interfaces.

```aura
class Rectangle implements Drawable {
    public width: number;
    public height: number;

    constructor(w: number, h: number) {
        this.width = w;
        this.height = h;
    }

    public function draw() {
        print "Drawing a rectangle...";
    }

    public function resize(factor: number) {
        this.width = this.width * factor;
        this.height = this.height * factor;
    }
}
```

## Multiple Interfaces

A class can implement multiple interfaces by separating them with a comma.

```aura
interface Loggable {
    function log(message: string): void;
}

class SystemComponent implements Drawable, Loggable {
    public function draw() { /* implementation */ }
    public function log(message: string) { /* implementation */ }
}
```

## Interface Inheritance

Interfaces can also extend other interfaces, combining multiple contracts into one.

```aura
interface AdvancedDrawable extends Drawable {
    function rotate(degrees: number): void;
}
```
