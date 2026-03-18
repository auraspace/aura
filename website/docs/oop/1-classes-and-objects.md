---
title: 1. Classes & Objects
sidebar_position: 1
---

# Classes & Objects

Aura is an object-oriented language that uses classes as the primary blueprint for creating objects. Classes encapsulate data (fields) and behavior (methods) into a single unit.

## Class Declaration

Use the `class` keyword to define a new class.

```aura
class Person {
    public name: string;
    public age: number;

    constructor(name: string, age: number) {
        this.name = name;
        this.age = age;
    }

    public function sayHello() {
        print "Hello, my name is " + this.name;
    }
}
```

## Access Modifiers

Aura supports three levels of encapsulation for class members:

-   **`public`** (default): Members are accessible from anywhere.
-   **`private`**: Members are only accessible within the class itself.
-   **`protected`**: Members are accessible within the class and its subclasses.

```aura
class BankAccount {
    private balance: number;

    constructor(initialBalance: number) {
        this.balance = initialBalance;
    }

    public function deposit(amount: number) {
        this.balance = this.balance + amount;
    }
}
```

## Constructors

The `constructor` keyword is used to define the initialization logic for an object. It is called when a new instance is created using the `new` keyword (optional in Aura, class name call acts as constructor).

```aura
let user = Person("Alice", 30);
user.sayHello();
```

## Static Members

Static fields and methods belong to the class itself rather than any specific instance. They are declared using the `static` keyword.

```aura
class MathUtils {
    public static PI: number = 3.14159;

    public static function square(x: number): number {
        return x * x;
    }
}

print MathUtils.PI;
print MathUtils.square(10);
```

## Inheritance

Aura supports single inheritance using the `extends` keyword. Subclasses inherit all public and protected members from their parent class.

```aura
class Employee extends Person {
    public employeeId: string;

    constructor(name: string, age: number, id: string) {
        super(name, age);
        this.employeeId = id;
    }

    public override function sayHello() {
        print "Hello, I am employee " + this.employeeId;
    }
}
```

-   **`super`**: Used to call the parent class constructor or methods.
-   **`override`**: Required when a subclass provides a new implementation for an inherited method.

## Abstract Classes

Abstract classes cannot be instantiated directly and are meant to be extended by other classes. They can contain abstract methods that must be implemented by subclasses.

```aura
abstract class Shape {
    public abstract function getArea(): number;
}

class Circle extends Shape {
    private radius: number;

    constructor(r: number) {
        this.radius = r;
    }

    public override function getArea(): number {
        return 3.14 * this.radius * this.radius;
    }
}
```
