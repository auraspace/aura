use aura_ast::*;
use crate::parse_file;

#[test]
fn parses_hello() {
    let src = r#"
package main

fun main() {
  println("Hello, Aura")
}
"#;
    let file = parse_file(src).expect("parse");
    assert_eq!(file.package.segments[0].name, "main");
    assert_eq!(file.functions.len(), 1);
    assert_eq!(file.functions[0].name.name, "main");
    assert_eq!(file.functions[0].body.stmts.len(), 1);
}

#[test]
fn parses_control_flow() {
    let src = r#"
package demo

fun add(a: Int, b: Int): Int {
  val sum: Int = a + b
  if (sum > 0) {
return sum
  } else {
return 0
  }
}
"#;
    let file = parse_file(src).expect("parse");
    assert_eq!(file.functions[0].params.len(), 2);
    assert!(file.functions[0].return_type.is_some());
    assert_eq!(file.functions[0].body.stmts.len(), 2);
}

#[test]
fn parses_while_and_nullable() {
    let src = r#"
package demo

fun loop(n: Int): Int {
  var i: Int = 0
  var s: String? = null
  while (i < n) {
i = i + 1
  }
  return i
}
"#;
    let file = parse_file(src).expect("parse");
    assert_eq!(file.functions[0].body.stmts.len(), 4);
}

#[test]
fn parses_assignment() {
    let src = r#"
package demo
fun main() {
  var x: Int = 1
  x = x + 1
}
"#;
    let file = parse_file(src).expect("parse");
    assert!(matches!(
        file.functions[0].body.stmts[1],
        Stmt::Expr(Expr::Assign(_))
    ));
}

#[test]
fn parses_class_and_method_call() {
    let src = r#"
package main

class Greeter(val name: String) {
  fun greet(): String {
return this.name
  }
}

fun main() {
  val g: Greeter = Greeter("Aura")
  println(g.greet())
}
"#;
    let file = parse_file(src).expect("parse");
    assert_eq!(file.classes.len(), 1);
    assert_eq!(file.classes[0].name.name, "Greeter");
    assert_eq!(file.classes[0].fields.len(), 1);
    assert_eq!(file.classes[0].methods.len(), 1);
    assert_eq!(file.functions.len(), 1);
    // second stmt is println(g.greet())
    match &file.functions[0].body.stmts[1] {
        Stmt::Expr(Expr::Call(c)) => match c.args[0].clone() {
            Expr::Call(inner) => {
                assert!(matches!(inner.callee.as_ref(), Expr::Field(_)));
            }
            other => panic!("expected method call, got {other:?}"),
        },
        other => panic!("expected call stmt, got {other:?}"),
    }
}

#[test]
fn parses_interface_and_implements() {
    let src = r#"
package main

interface Named {
  fun name(): String
}

class User(val n: String) : Named {
  fun name(): String {
return this.n
  }
}

fun show(x: Named) {
  println(x.name())
}
"#;
    let file = parse_file(src).expect("parse");
    assert_eq!(file.interfaces.len(), 1);
    assert_eq!(file.interfaces[0].methods.len(), 1);
    assert_eq!(file.classes[0].implements.len(), 1);
    assert_eq!(file.classes[0].implements[0].name, "Named");
}

#[test]
fn parses_force_unwrap() {
    let src = r#"
package t
fun f(x: String?): String {
  return x!!
}
"#;
    let file = parse_file(src).expect("parse");
    match &file.functions[0].body.stmts[0] {
        Stmt::Return(r) => {
            assert!(matches!(r.value, Some(Expr::ForceUnwrap(_))));
        }
        other => panic!("expected return, got {other:?}"),
    }
}

#[test]
fn parses_generic_class_and_ctor() {
    let src = r#"
package main

class Box<T>(val value: T) {
  fun get(): T {
return this.value
  }
}

fun id<T>(x: T): T {
  return x
}

fun main() {
  val b: Box<String> = Box<String>("hi")
  println(b.get())
  println(id<String>("ok"))
}
"#;
    let file = parse_file(src).expect("parse");
    assert_eq!(file.classes[0].type_params.len(), 1);
    assert_eq!(file.functions[0].type_params.len(), 1);
    assert_eq!(file.functions[0].name.name, "id");
    // first stmt init is Call with type_args
    match &file.functions[1].body.stmts[0] {
        Stmt::Var(v) => match &v.init {
            Expr::Call(c) => {
                assert_eq!(c.type_args.len(), 1);
                assert_eq!(c.type_args[0].name.name, "String");
            }
            other => panic!("expected call, got {other:?}"),
        },
        other => panic!("expected var, got {other:?}"),
    }
}

#[test]
fn parses_test_attr() {
    let src = r#"
package main
@test
fun adds() {
  assert_eq(1, 1)
}
"#;
    let file = parse_file(src).expect("parse");
    assert!(file.functions[0].is_test);
    assert_eq!(file.functions[0].name.name, "adds");
}

#[test]
fn parses_try_throw() {
    let src = r#"
package main
fun f() {
  try {
throw "x"
  } catch (e: String) {
println(e)
  } finally {
println("done")
  }
}
"#;
    let file = parse_file(src).expect("parse");
    match &file.functions[0].body.stmts[0] {
        Stmt::Try(t) => {
            assert!(t.catch.is_some());
            assert!(t.finally.is_some());
            match &t.try_block.stmts[0] {
                Stmt::Throw(_) => {}
                other => panic!("expected throw, got {other:?}"),
            }
        }
        other => panic!("expected try, got {other:?}"),
    }
}

#[test]
fn parses_enum_and_match() {
    let src = r#"
package main
enum Result<T, E> {
  case Ok(value: T)
  case Err(error: E)
}
fun f(r: Result<Int, String>) {
  match (r) {
case Ok(v) => { return }
case Err(e) => { println(e) }
  }
}
"#;
    let file = parse_file(src).expect("parse");
    assert_eq!(file.enums.len(), 1);
    assert_eq!(file.enums[0].variants.len(), 2);
    assert_eq!(file.enums[0].variants[0].fields.len(), 1);
    match &file.functions[0].body.stmts[0] {
        Stmt::Match(m) => assert_eq!(m.arms.len(), 2),
        other => panic!("expected match, got {other:?}"),
    }
}

#[test]
fn parses_struct() {
    let src = r#"
package main
struct Point(val x: Int, val y: Int) {
  fun sum(): Int {
return this.x + this.y
  }
}
"#;
    let file = parse_file(src).expect("parse");
    assert_eq!(file.classes.len(), 1);
    assert_eq!(file.classes[0].kind, NominalKind::Struct);
    assert_eq!(file.classes[0].name.name, "Point");
    assert_eq!(file.classes[0].fields.len(), 2);
    assert_eq!(file.classes[0].methods.len(), 1);
}

#[test]
fn rejects_struct_implements() {
    let src = r#"
package main
interface Named { fun name(): String }
struct S(val n: String) : Named {
  fun name(): String { return this.n }
}
"#;
    let err = parse_file(src).expect_err("struct implements");
    assert!(err.message.contains("struct"), "{}", err.message);
}

#[test]
fn parses_type_param_bounds_and_where() {
    let src = r#"
package main

interface Named {
  fun name(): String
}

interface Id {
  fun id(): Int
}

fun greet<T : Named>(x: T): String {
  return x.name()
}

fun both<T>(x: T) where T : Named, T : Id {
  println(x.name())
}

class Holder<T>(val item: T) where T : Named {
  fun label(): String {
return this.item.name()
  }
}
"#;
    let file = parse_file(src).expect("parse");
    assert_eq!(file.functions[0].type_params[0].name.name, "T");
    assert_eq!(file.functions[0].type_params[0].bounds.len(), 1);
    assert_eq!(file.functions[0].type_params[0].bounds[0].name, "Named");
    assert_eq!(file.functions[1].type_params[0].bounds.len(), 2);
    assert_eq!(file.functions[1].type_params[0].bounds[0].name, "Named");
    assert_eq!(file.functions[1].type_params[0].bounds[1].name, "Id");
    assert_eq!(file.classes[0].type_params[0].bounds.len(), 1);
    assert_eq!(file.classes[0].type_params[0].bounds[0].name, "Named");
}

#[test]
fn parses_import_and_pub() {
    let src = r#"
package demo.app

import demo.math
import demo.other as O

pub fun square(x: Int): Int {
  return x * x
}

fun private_helper(): Int {
  return 1
}
"#;
    let file = parse_file(src).expect("parse");
    assert_eq!(file.imports.len(), 2);
    assert_eq!(file.imports[0].path.display(), "demo.math");
    assert!(file.imports[0].alias.is_none());
    assert_eq!(file.imports[1].path.display(), "demo.other");
    assert_eq!(file.imports[1].alias.as_ref().unwrap().name, "O");
    assert!(file.functions[0].is_pub);
    assert!(!file.functions[1].is_pub);
}

#[test]
fn rejects_missing_package() {
    let err = parse_file("fun main() {}").unwrap_err();
    assert!(err.message.contains("package"));
}
