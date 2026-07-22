use crate::parse_file;
use aura_ast::*;

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
fn parses_explicit_foreign_declaration_metadata() {
    let file = parse_file(
        "package demo\n@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\nextern \"C\" fun native_abs(value: Int): Int\n",
    )
    .expect("foreign declaration parses");
    let foreign = &file.foreign_functions[0];
    assert_eq!(foreign.name.name, "native_abs");
    assert!(matches!(foreign.convention, ForeignCallingConvention::C));
    assert_eq!(foreign.library.as_ref().unwrap().name, "m");
    assert_eq!(foreign.target.as_ref().unwrap().triple, "native");
    assert_eq!(foreign.abi.as_ref().unwrap().identity, "c");
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
fn parses_scoped_ref_type() {
    let file = parse_file("package demo\nfun borrow(x: ref String): ref String { return x }\n")
        .expect("parse");
    let param = &file.functions[0].params[0].ty;
    assert!(param.reference);
    assert_eq!(param.name.name, "String");
    assert!(file.functions[0].return_type.as_ref().unwrap().reference);
}

#[test]
fn rejects_nested_ref_syntax() {
    let err = parse_file("package demo\nfun bad(x: ref ref String) {}\n").expect_err("nested ref");
    assert!(err.message.contains("nested `ref`"));
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
    assert_eq!(file.classes[0].implements[0].name.name, "Named");
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

#[test]
fn parses_for_range() {
    let src = r#"
package t
fun main() {
  for (i in 0..n) {
    println("x")
  }
}
"#;
    let file = parse_file(src).expect("parse");
    match &file.functions[0].body.stmts[0] {
        Stmt::ForRange(f) => {
            assert_eq!(f.name.name, "i");
            assert!(!f.inclusive);
        }
        other => panic!("expected ForRange, got {other:?}"),
    }
}

#[test]
fn parses_for_range_inclusive() {
    let src = r#"
package t
fun main() {
  for (i in 1..=3) {
    println("x")
  }
}
"#;
    let file = parse_file(src).expect("parse");
    match &file.functions[0].body.stmts[0] {
        Stmt::ForRange(f) => {
            assert_eq!(f.name.name, "i");
            assert!(f.inclusive);
        }
        other => panic!("expected inclusive ForRange, got {other:?}"),
    }
}

#[test]
fn parses_for_in() {
    let src = r#"
package t
fun main() {
  for (x in a) {
    println("x")
  }
}
"#;
    let file = parse_file(src).expect("parse");
    match &file.functions[0].body.stmts[0] {
        Stmt::ForIn(f) => {
            assert_eq!(f.name.name, "x");
        }
        other => panic!("expected ForIn, got {other:?}"),
    }
}

#[test]
fn parses_break_continue() {
    let src = r#"
package t
fun main() {
  while (true) {
    break
    continue
  }
}

"#;
    let file = parse_file(src).expect("parse");
    let body = &file.functions[0].body.stmts[0];
    match body {
        Stmt::While(w) => {
            assert!(matches!(w.body.stmts[0], Stmt::Break(_)));
            assert!(matches!(w.body.stmts[1], Stmt::Continue(_)));
        }
        other => panic!("expected while, got {other:?}"),
    }
}

#[test]
fn parses_async_function_and_await() {
    let file =
        parse_file("package demo\nasync fun load(id: Int): User { return await fetch(id) }\n")
            .expect("parse");
    assert_eq!(file.async_functions.len(), 1);
    let fun = &file.async_functions[0];
    assert_eq!(fun.name.name, "load");
    assert_eq!(fun.params.len(), 1);
    match &fun.body.stmts[0] {
        Stmt::Return(ReturnStmt {
            value: Some(Expr::Async(AsyncExpr::Await(await_expr))),
            ..
        }) => {
            assert!(matches!(await_expr.operand.as_ref(), Expr::Call(_)));
            assert_eq!(await_expr.span, Span::new(52, 67));
        }
        other => panic!("expected await return, got {other:?}"),
    }
}

#[test]
fn rejects_async_without_fun() {
    let err = parse_file("package demo\nasync class Worker {}\n").expect_err("invalid async");
    assert!(err.message.contains("`fun` after `async`"));
    assert_eq!(err.span.start, 19);
}

#[test]
fn parses_task_and_channel_operations() {
    let file = parse_file(
        "package demo\nfun main() {\n  val ch = Channel<Int>(2)\n  val task = spawn { ch.send(1) }\n  val value = ch.receive()\n  join(task)\n  cancel(task)\n  ch.close()\n}\n",
    )
    .expect("parse");
    let body = &file.functions[0].body.stmts;
    assert!(matches!(
        body[0],
        Stmt::Var(VarStmt {
            init: Expr::Async(AsyncExpr::ChannelCreate(_)),
            ..
        })
    ));
    assert!(matches!(
        body[1],
        Stmt::Var(VarStmt {
            init: Expr::Async(AsyncExpr::Spawn(_)),
            ..
        })
    ));
    assert!(matches!(
        body[2],
        Stmt::Var(VarStmt {
            init: Expr::Async(AsyncExpr::ChannelReceive(_)),
            ..
        })
    ));
    assert!(matches!(
        body[3],
        Stmt::Expr(Expr::Async(AsyncExpr::Join(_)))
    ));
    assert!(matches!(
        body[4],
        Stmt::Expr(Expr::Async(AsyncExpr::Cancel(_)))
    ));
    assert!(matches!(
        body[5],
        Stmt::Expr(Expr::Async(AsyncExpr::ChannelClose(_)))
    ));
}

#[test]
fn allows_join_as_function_and_qualified_name() {
    let file = parse_file(
        "package demo\nimport std.join\npub fun join(value: String): String { return value }\nfun call(parts: Array<String>, sep: String): String { return join(parts, sep) }\n",
    )
    .expect("join remains usable as an identifier in declarations and paths");

    assert_eq!(file.imports[0].path.segments[1].name, "join");
    assert_eq!(file.functions[0].name.name, "join");
    assert!(file.functions[0].is_pub);
    assert!(matches!(
        file.functions[1].body.stmts[0],
        Stmt::Return(ReturnStmt {
            value: Some(Expr::Call(_)),
            ..
        })
    ));
}

#[test]
fn rejects_malformed_task_and_channel_operations() {
    for (src, expected) in [
        (
            "package demo\nfun main() { join() }\n",
            "expected expression",
        ),
        (
            "package demo\nfun main() { cancel(a, b) }\n",
            "expected `)`",
        ),
        (
            "package demo\nfun main() { Channel<Int>() }\n",
            "capacity argument",
        ),
        (
            "package demo\nfun main() { ch.send() }\n",
            "requires 1 argument",
        ),
    ] {
        let err = parse_file(src).expect_err("malformed async operation");
        assert!(err.message.contains(expected), "{src}: {}", err.message);
    }
}
