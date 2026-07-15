use crate::check_file;
use crate::ty::Ty;
use aura_parser::parse_file;

#[test]
fn mono_suffix() {
    let t = Ty::ClassApp {
        name: "Box".into(),
        args: vec![Ty::String],
    };
    assert_eq!(t.mono_suffix(), "Box_String");
}

#[test]
fn try_catch_typechecks() {
    let src = r#"
package t
fun boom() { throw "x" }
fun main() {
  try {
boom()
  } catch (e: String) {
println(e)
  }
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check");
}

#[test]
fn throw_rejects_unit() {
    let src = r#"
package t
fun main() {
  throw null
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("throw null");
    assert!(err.message.contains("throw") || err.message.contains("Null"), "{}", err.message);
}

#[test]
fn result_enum_and_match() {
    let src = r#"
package t
enum Result<T, E> {
  case Ok(value: T)
  case Err(error: E)
}
fun f(): Result<Int, String> {
  return Ok(1)
}
fun g(r: Result<Int, String>): Int {
  match (r) {
case Ok(v) => { return v }
case Err(e) => { return 0 }
  }
}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("check");
    assert!(checked.enums.iter().any(|e| e.name == "Result"));
    assert!(checked
        .mono_enums
        .iter()
        .any(|(n, a)| n == "Result" && a == &[Ty::Int, Ty::String]));
}

#[test]
fn match_nonexhaustive_errors() {
    let src = r#"
package t
enum Color { case Red case Green }
fun f(c: Color) {
  match (c) {
case Red => { println("r") }
  }
}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("non-exhaustive");
    assert!(err.message.contains("non-exhaustive") || err.message.contains("Green"), "{}", err.message);
}

#[test]
fn struct_fields_and_methods() {
    let src = r#"
package t
struct Point(val x: Int, val y: Int) {
  fun sum(): Int { return this.x + this.y }
}
fun f(): Int {
  val p: Point = Point(1, 2)
  return p.sum()
}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("check");
    assert!(checked.classes.iter().any(|c| c.is_struct && c.name == "Point"));
}

#[test]
fn bounds_allow_method_on_type_param() {
    let src = r#"
package t
interface Named {
  fun name(): String
}
class User(val n: String) : Named {
  fun name(): String { return this.n }
}
fun greet<T : Named>(x: T): String {
  return x.name()
}
fun main() {
  val s: String = greet(User("hi"))
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("bounded type param method call");
}

#[test]
fn where_multi_bounds_and_reject_unsatisfied() {
    let src_ok = r#"
package t
interface Named { fun name(): String }
interface Id { fun id(): Int }
class Both(val n: String, val i: Int) : Named, Id {
  fun name(): String { return this.n }
  fun id(): Int { return this.i }
}
fun f<T>(x: T) where T : Named, T : Id {
  println(x.name())
}
fun main() { f(Both("a", 1)) }
"#;
    let file = parse_file(src_ok).expect("parse");
    check_file(&file).expect("multi bounds ok");

    let src_bad = r#"
package t
interface Named { fun name(): String }
interface Id { fun id(): Int }
class OnlyNamed(val n: String) : Named {
  fun name(): String { return this.n }
}
fun f<T>(x: T) where T : Named, T : Id {
  println(x.name())
}
fun main() { f(OnlyNamed("a")) }
"#;
    let file = parse_file(src_bad).expect("parse");
    let err = check_file(&file).expect_err("should reject missing Id bound");
    assert!(
        err.message.contains("Id") || err.message.contains("bound"),
        "unexpected: {}",
        err.message
    );
}

#[test]
fn unbounded_type_param_cannot_call_methods() {
    let src = r#"
package t
interface Named { fun name(): String }
fun bad<T>(x: T): String {
  return x.name()
}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("unbounded T");
    assert!(
        err.message.contains("unbounded") || err.message.contains("method"),
        "unexpected: {}",
        err.message
    );
}

#[test]
fn null_flow_narrows_in_if() {
    let src = r#"
package t
fun f(name: String?): String {
  if (name != null) {
return name
  } else {
return "x"
  }
}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check should allow name after != null");
}

#[test]
fn null_flow_rejects_without_check() {
    let src = r#"
package t
fun f(name: String?): String {
  return name
}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("should reject String? as String");
    assert!(err.message.contains("return type mismatch") || err.message.contains("String"));
}

#[test]
fn infers_box_and_id_type_args() {
    let src = r#"
package t
class Box<T>(val value: T) {
  fun get(): T { return this.value }
}
fun id<T>(x: T): T { return x }
fun main() {
  val a = Box("hi")
  val b: Box<String> = Box("x")
  id("y")
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("check");
    assert!(
        checked
            .mono_classes
            .iter()
            .any(|(n, a)| n == "Box" && a == &[Ty::String])
    );
    assert!(
        checked
            .mono_funs
            .iter()
            .any(|(n, a)| n == "id" && a == &[Ty::String])
    );
    assert!(!checked.call_instantiations.is_empty());
}

#[test]
fn import_allows_pub_function() {
    use aura_ast::ImportDecl;
    let mut lib = parse_file(
        r#"
package demo.math
pub fun square(x: Int): Int { return x * x }
fun mul(a: Int, b: Int): Int { return a * b }
"#,
    )
    .expect("parse lib");
    for f in &mut lib.functions {
        f.origin_package = "demo.math".into();
    }
    let mut app = parse_file(
        r#"
package demo.app
import demo.math
fun main() {
  square(3)
}
"#,
    )
    .expect("parse app");
    for f in &mut app.functions {
        f.origin_package = "demo.app".into();
    }
    for i in &mut app.imports {
        i.origin_package = "demo.app".into();
    }
    // Merge lib into app unit
    app.functions.extend(lib.functions);
    app.interfaces.extend(lib.interfaces);
    app.enums.extend(lib.enums);
    app.classes.extend(lib.classes);
    let _ = ImportDecl {
        path: app.imports[0].path.clone(),
        alias: None,
        origin_package: "demo.app".into(),
        span: app.imports[0].span,
    };
    check_file(&app).expect("cross-package pub call");
}

#[test]
fn import_rejects_private_function() {
    let mut lib = parse_file(
        r#"
package demo.math
fun mul(a: Int, b: Int): Int { return a * b }
"#,
    )
    .expect("parse lib");
    for f in &mut lib.functions {
        f.origin_package = "demo.math".into();
    }
    let mut app = parse_file(
        r#"
package demo.app
import demo.math
fun main() {
  mul(2, 3)
}
"#,
    )
    .expect("parse app");
    for f in &mut app.functions {
        f.origin_package = "demo.app".into();
    }
    for i in &mut app.imports {
        i.origin_package = "demo.app".into();
    }
    app.functions.extend(lib.functions);
    let err = check_file(&app).expect_err("private");
    assert!(
        err.message.contains("private") || err.message.contains("mul"),
        "{}",
        err.message
    );
}

#[test]
fn class_throw_and_catch_typechecks() {
    let src = r#"
package t
class Error(val msg: String) {}
fun boom() { throw Error("x") }
fun main() {
  try {
    boom()
  } catch (e: Error) {
    println(e.msg)
  }
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check");
}

#[test]
fn reject_throw_interface() {
    let src = r#"
package t
interface I { fun m(): Int }
fun main() {
  throw null
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("throw null");
    assert!(err.message.contains("throw") || err.message.contains("Null"), "{}", err.message);
}

#[test]
fn for_range_typechecks() {
    let src = r#"
package t
fun main() {
  var s: Int = 0
  for (i in 0..5) {
    s = s + i
  }
  for (j in 1..=3) {
    s = s + j
  }
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check");
}

#[test]
fn for_range_rejects_non_int() {
    let src = r#"
package t
fun main() {
  for (i in "a".."b") {}
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("non-int range");
    assert!(err.message.contains("Int"), "{}", err.message);
}

#[test]
fn break_continue_in_loop_ok() {
    let src = r#"
package t
fun main() {
  for (i in 0..3) {
    if (i == 0) { continue }
    if (i == 2) { break }
  }
  while (true) {
    break
  }
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check");
}

#[test]
fn break_outside_loop_errors() {
    let src = r#"
package t
fun main() {
  break
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("break outside");
    assert!(err.message.contains("break"), "{}", err.message);
}

#[test]
fn array_int_typechecks() {
    let src = r#"
package t
fun main() {
  val a: Array<Int> = Array(3)
  a.set(0, 1)
  val x: Int = a.get(0)
  val n: Int = a.len
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("check");
    assert!(
        checked
            .mono_classes
            .iter()
            .any(|(n, a)| n == "Array" && a == &[Ty::Int])
    );
}

#[test]
fn for_in_array_typechecks() {
    let src = r#"
package t
fun main() {
  val a: Array<Int> = Array(2)
  a.set(0, 1)
  a.set(1, 2)
  var s: Int = 0
  for (x in a) {
    s = s + x
  }
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check");
}

#[test]
fn for_in_rejects_non_array() {
    let src = r#"
package t
fun main() {
  for (x in 1) {}
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("non-array for-in");
    assert!(err.message.contains("Array"), "{}", err.message);
}

#[test]
fn array_rejects_class_elem() {
    let src = r#"
package t
class Box(val x: Int) {}
fun main() {
  val a: Array<Box> = Array(1)
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("class elem");
    assert!(
        err.message.contains("Array") || err.message.contains("Int"),
        "{}",
        err.message
    );
}
