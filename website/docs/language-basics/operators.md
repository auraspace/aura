---
title: Expressions & Operators
sidebar_position: 4
---

# Expressions & Operators

Aura provides a comprehensive set of operators for performing arithmetic, comparisons, logical operations, and more.

## Arithmetic Operators

Aura supports standard arithmetic operations:

| Operator | Name           | Example    | Result |
| :------- | :------------- | :--------- | :----- |
| `+`      | Addition       | `5 + 3`    | `8`    |
| `-`      | Subtraction    | `10 - 4`   | `6`    |
| `*`      | Multiplication | `2 * 3`    | `6`    |
| `/`      | Division       | `15 / 3`   | `5`    |
| `%`      | Remainder      | `10 % 3`   | `1`    |

## Comparison Operators

Comparison operators evaluate to a `boolean` (`true` or `false`):

| Operator | Description              | Example    |
| :------- | :----------------------- | :--------- |
| `==`      | Equal to                 | `5 == 5`   |
| `!=`      | Not equal to             | `5 != 3`   |
| `<`       | Less than                | `2 < 10`   |
| `<=`      | Less than or equal to    | `5 <= 5`   |
| `>`       | Greater than             | `10 > 2`   |
| `>=`      | Greater than or equal to | `10 >= 10` |

## Logical Operators

Logical operators are used to determine logic between variables or values:

| Operator | Name     | Example            |
| :------- | :------- | :----------------- |
| `&&`     | AND      | `true && false`    |
| `||`     | OR       | `true || false`    |
| `!`      | NOT      | `!true`            |

## Bitwise Operators

For low-level data manipulation, Aura provides bitwise operators:

| Operator | Name        |
| :------- | :---------- |
| `&`      | AND         |
| `|`      | OR          |
| `^`      | XOR         |
| `~`      | NOT (Unary) |
| `<<`     | SHL (Left)  |
| `>>`     | SHR (Right) |

## Literal Expressions

Aura supports several types of literal notation:

- **Template Literals**: Using backticks to embed expressions: `` `Value: ${expr}` ``.
- **Array Literals**: Using square brackets: `[1, 2, 3]`.
- **Null Literal**: Explicit absence of value: `null`.

## Postfix Operators

- **Member Access**: `obj.member`
- **Index Access**: `arr[index]`
- **Function Call**: `func(args)`
