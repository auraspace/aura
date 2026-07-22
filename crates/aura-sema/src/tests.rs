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
fn scoped_ref_types_allow_lexical_local_and_parameter_use() {
    let file = parse_file(
        "package t\nfun borrow(x: ref String) { val y: ref String = x println(y) }\nfun main() {}\n",
    )
    .expect("parse");
    check_file(&file).expect("scoped ref type");
}

#[test]
fn scoped_ref_rejects_returns_and_lambda_captures() {
    let returned =
        parse_file("package t\nfun bad(x: ref String): ref String { return x }\n").expect("parse");
    let err = check_file(&returned).expect_err("ref return");
    assert!(err.primary().message.contains("cannot be returned"));

    let captured =
        parse_file("package t\nfun bad(x: String) { val y: ref String = x val f = () => y }\n")
            .expect("parse");
    let err = check_file(&captured).expect_err("ref capture");
    assert!(err.primary().message.contains("cannot capture borrow"));
}

#[test]
fn scoped_ref_rejects_assignment_into_longer_lived_binding() {
    let file = parse_file(
        "package t\nfun bad() { var out: String = \"out\" if (true) { val x: String = \"inner\" val y: ref String = x out = y } }\n",
    )
    .expect("parse");
    let err = check_file(&file).expect_err("borrow escape through assignment");
    assert!(err.primary().message.contains("escape"));
}

#[test]
fn scoped_ref_rejects_owning_field_storage() {
    let file = parse_file("package t\nclass Holder(val value: ref String) {}\nfun main() {}\n")
        .expect("parse");
    let err = check_file(&file).expect_err("ref field");
    assert!(err.primary().message.contains("stored in fields"));
}

#[test]
fn scoped_ref_allows_array_field_view_without_return_escape() {
    let file = parse_file(
        "package t\nclass Holder(val items: Array<Int>) {\n  fun view() { val xs: ref Array<Int> = this.items val n: Int = xs.len }\n}\nfun main() {}\n",
    )
    .expect("parse");
    check_file(&file).expect("Array field borrow view");
}

#[test]
fn scoped_ref_allows_read_only_collection_iteration_views() {
    let file = parse_file(
        r#"
package t
class Snapshot(val items: Array<Int>) {
  fun len(): Int { return this.items.len }
  fun get(i: Int): Int { return this.items.get(i) }
}
fun inspect(snapshot: Snapshot) {
  val iterator: ref Snapshot = snapshot
  val items: ref Array<Int> = iterator.items
  for (item in items) {
    val current: ref Int = item
    if (current == 1 || current == 2) { }
  }
}
fun main() {}
"#,
    )
    .expect("parse");
    check_file(&file).expect("snapshot iteration borrow should stay lexical");
}

#[test]
fn scoped_ref_rejects_collection_iteration_escape() {
    let returned = parse_file(
        r#"
package t
class Snapshot(val items: Array<Int>) {}
fun bad(snapshot: Snapshot): ref Array<Int> {
  val iterator: ref Snapshot = snapshot
  val items: ref Array<Int> = iterator.items
  return items
}
fun main() {}
"#,
    )
    .expect("parse");
    let err = check_file(&returned).expect_err("iterator borrow return");
    assert!(err.primary().message.contains("cannot be returned"));

    let captured = parse_file(
        r#"
package t
class Snapshot(val items: Array<Int>) {}
fun bad(snapshot: Snapshot) {
  val iterator: ref Snapshot = snapshot
  val items: ref Array<Int> = iterator.items
  val f = () => items.len
}
fun main() {}
"#,
    )
    .expect("parse");
    let err = check_file(&captured).expect_err("iterator borrow capture");
    assert!(err.primary().message.contains("cannot capture borrow"));
}

#[test]
fn scoped_ref_rejects_nullable_targets() {
    let file =
        parse_file("package t\nfun bad(x: ref String?): String { return \"x\" }\n").expect("parse");
    let err = check_file(&file).expect_err("nullable ref");
    assert!(err.primary().message.contains("must be non-null"));
}

#[test]
fn scoped_ref_rejects_mutable_bindings_and_function_targets() {
    let mutable =
        parse_file("package t\nfun bad(x: String) { var y: ref String = x }\nfun main() {}\n")
            .expect("parse");
    let err = check_file(&mutable).expect_err("mutable ref binding");
    assert!(err.primary().message.contains("must be immutable"));

    let function =
        parse_file("package t\nfun bad(x: ref (Int) -> Int): Int { return 0 }\nfun main() {}\n")
            .expect("parse");
    let err = check_file(&function).expect_err("function ref");
    assert!(err.primary().message.contains("function types"));
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
    assert!(
        err.primary().message.contains("throw") || err.primary().message.contains("Null"),
        "{}",
        err.primary().message
    );
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
    assert!(
        err.primary().message.contains("non-exhaustive") || err.primary().message.contains("Green"),
        "{}",
        err.primary().message
    );
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
    assert!(checked
        .classes
        .iter()
        .any(|c| c.is_struct && c.name == "Point"));
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
fn hashable_primitives_satisfy_bound_and_hash_method() {
    let src = r#"
package t
interface Hashable { fun hash(): Int }
fun hash_it<T : Hashable>(x: T): Int { return x.hash() }
fun main() {
  val a: Int = hash_it(7)
  val b: Int = hash_it("a")
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("primitive Hashable bound");
}

#[test]
fn hashable_rejects_bool() {
    let src = r#"
package t
interface Hashable { fun hash(): Int }
fun hash_it<T : Hashable>(x: T): Int { return x.hash() }
fun main() { val a: Int = hash_it(true) }
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("Bool must not satisfy Hashable");
    assert!(err
        .primary()
        .message
        .contains("does not satisfy bound `Hashable`"));
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
        err.primary().message.contains("Id") || err.primary().message.contains("bound"),
        "unexpected: {}",
        err.primary().message
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
        err.primary().message.contains("unbounded") || err.primary().message.contains("method"),
        "unexpected: {}",
        err.primary().message
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
    assert!(
        err.primary().message.contains("return type mismatch")
            || err.primary().message.contains("String")
    );
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
    assert!(checked
        .mono_classes
        .iter()
        .any(|(n, a)| n == "Box" && a == &[Ty::String]));
    assert!(checked
        .mono_funs
        .iter()
        .any(|(n, a)| n == "id" && a == &[Ty::String]));
    assert!(!checked.call_instantiations.is_empty());
}

#[test]
fn nested_mono_skips_open_type_params() {
    // C4u: Wrapper<T> field Box<T> must not record open Box_T monomorphs.
    let src = r#"
package t
class Box<T>(val value: T) {
  fun get(): T { return this.value }
}
class Wrapper<T>(val inner: Box<T>) {
  fun unwrap(): T { return this.inner.get() }
}
fun main() {
  val w: Wrapper<String> = Wrapper(Box("x"))
  w.unwrap()
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("check");
    assert!(
        checked
            .mono_classes
            .iter()
            .any(|(n, a)| n == "Wrapper" && a == &[Ty::String]),
        "expected Wrapper_String"
    );
    assert!(
        checked
            .mono_classes
            .iter()
            .any(|(n, a)| n == "Box" && a == &[Ty::String]),
        "expected Box_String from nested expand"
    );
    assert!(
        !checked
            .mono_classes
            .iter()
            .any(|(_, a)| a.iter().any(|t| t.is_open())),
        "open monomorphs must not be recorded: {:?}",
        checked.mono_classes
    );
}

#[test]
fn nested_mono_expands_generic_method_signature_types() {
    let src = r#"
package t
class Entry<K, V>(val key: K, val value: V) {}
class Table<K, V>(val key: K, val value: V) {
  fun entries(): Array<Entry<K, V>> { return Array(0) }
}
fun main() {
  val table = Table<Int, String>(1, "one")
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("check");
    let entry = Ty::ClassApp {
        name: "Entry@t".into(),
        args: vec![Ty::Int, Ty::String],
    };
    assert!(
        checked
            .mono_classes
            .iter()
            .any(|(n, args)| n == "Entry" && args == &[Ty::Int, Ty::String]),
        "expected Entry<Int, String>, got {:?}",
        checked.mono_classes
    );
    assert!(
        checked
            .mono_classes
            .iter()
            .any(|(n, args)| n == "Array" && args.as_slice() == std::slice::from_ref(&entry)),
        "expected Array<Entry<Int, String>>, got {:?}",
        checked.mono_classes
    );
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
fn same_fun_name_two_packages_via_alias() {
    let mut a = parse_file(
        r#"
package demo.a
pub fun add(x: Int, y: Int): Int { return x + y }
"#,
    )
    .expect("parse a");
    for f in &mut a.functions {
        f.origin_package = "demo.a".into();
    }
    let mut b = parse_file(
        r#"
package demo.b
pub fun add(x: Int, y: Int): Int { return x * y }
"#,
    )
    .expect("parse b");
    for f in &mut b.functions {
        f.origin_package = "demo.b".into();
    }
    let mut app = parse_file(
        r#"
package demo.app
import demo.a as A
import demo.b as B
fun main() {
  A.add(1, 2)
  B.add(1, 2)
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
    app.functions.extend(a.functions);
    app.functions.extend(b.functions);
    check_file(&app).expect("same name two packages");
}

#[test]
fn import_alias_qualified_call() {
    let mut lib = parse_file(
        r#"
package demo.math
pub fun square(x: Int): Int { return x * x }
"#,
    )
    .expect("parse lib");
    for f in &mut lib.functions {
        f.origin_package = "demo.math".into();
    }
    let mut app = parse_file(
        r#"
package demo.app
import demo.math as Math
fun main() {
  Math.square(3)
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
    check_file(&app).expect("alias qualified call");
}

#[test]
fn import_alias_qualified_type() {
    let mut lib = parse_file(
        r#"
package demo.math
pub class Point(val x: Int, val y: Int) {}
"#,
    )
    .expect("parse lib");
    for c in &mut lib.classes {
        c.origin_package = "demo.math".into();
    }
    let mut app = parse_file(
        r#"
package demo.app
import demo.math as Math
fun main() {
  val p: Math.Point = Math.Point(1, 2)
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
    app.classes.extend(lib.classes);
    check_file(&app).expect("alias qualified type");
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
        err.primary().message.contains("private") || err.primary().message.contains("mul"),
        "{}",
        err.primary().message
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
    assert!(
        err.primary().message.contains("throw") || err.primary().message.contains("Null"),
        "{}",
        err.primary().message
    );
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
    assert!(
        err.primary().message.contains("Int"),
        "{}",
        err.primary().message
    );
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
    assert!(
        err.primary().message.contains("break"),
        "{}",
        err.primary().message
    );
}

#[test]
fn array_accepts_enum_elem() {
    // C6g: Array of enum elements by value.
    let src = r#"
package t
enum Color { Red, Green }
fun main() {
  val a: Array<Color> = Array(1)
  a.set(0, Red())
  val c: Color = a.get(0)
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("enum Array elem");
}

#[test]
fn array_rejects_interface_elem_clearly() {
    // C4x: dedicated diagnostic for Array of interface.
    let src = r#"
package t
interface Named {
  fun name(): String
}
fun main() {
  val a: Array<Named> = Array(1)
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("array of interface");
    assert!(
        err.primary().message.contains("interface") && err.primary().message.contains("Named"),
        "{}",
        err.primary().message
    );
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
    assert!(checked
        .mono_classes
        .iter()
        .any(|(n, a)| n == "Array" && a == &[Ty::Int]));
}

#[test]
fn array_push_typechecks() {
    let src = r#"
package t
fun main() {
  val a: Array<Int> = Array(0)
  a.push(1)
  a.push(2)
  val n: Int = a.len
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check");
}

#[test]
fn array_pop_typechecks() {
    let src = r#"
package t
fun main() {
  val a: Array<Int> = Array(0)
  a.push(1)
  a.push(2)
  val x: Int = a.pop()
  val n: Int = a.len
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check");
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
    assert!(
        err.primary().message.contains("Array") || err.primary().message.contains("String"),
        "{}",
        err.primary().message
    );
}

#[test]
fn for_in_string_typechecks() {
    let src = r#"
package t
fun main() {
  var s: Int = 0
  for (b in "ab") {
    s = s + b
  }
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("for-in string");
}

#[test]
fn undefined_name_suggests_similar() {
    // C5c: typo hint.
    let src = r#"
package t
fun main() {
  val count: Int = 1
  println(cout)
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("undefined");
    assert!(
        err.primary().message.contains("undefined name") && err.primary().message.contains("count"),
        "{}",
        err.primary().message
    );
}

#[test]
fn multi_error_collects_body_errors() {
    // C6h: two undefined names in one body → two diagnostics.
    let src = r#"
package t
fun main() {
  println(missing_one)
  println(missing_two)
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("multi");
    assert!(
        err.errors.len() >= 2,
        "expected ≥2 errors, got {}: {:?}",
        err.errors.len(),
        err.errors
    );
    let joined = err
        .errors
        .iter()
        .map(|e| e.message.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        joined.contains("missing_one") && joined.contains("missing_two"),
        "{joined}"
    );
}

#[test]
fn for_in_duck_len_get() {
    // C4y: class with len field + get(i).
    let src = r#"
package t
class R(val len: Int) {
  fun get(i: Int): Int { return i }
}
fun main() {
  var s: Int = 0
  for (x in R(2)) { s = s + x }
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("duck for-in");
}

#[test]
fn for_in_iface_iterable() {
    // C6c: interface with len() + get(i).
    let src = r#"
package t
interface Iterable {
  fun len(): Int
  fun get(i: Int): Int
}
class R(val n: Int) : Iterable {
  fun len(): Int { return this.n }
  fun get(i: Int): Int { return i }
}
fun sum(it: Iterable): Int {
  var s: Int = 0
  for (x in it) { s = s + x }
  return s
}
fun main() {
  val r = R(2)
  val n = sum(r)
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("iface for-in");
}

#[test]
fn generic_iface_implements_mono() {
    // C8c: interface Boxable<T>; class implements Boxable<Int>.
    let src = r#"
package t
interface Boxable<T> {
  fun get(): T
}
class IntBox(val n: Int) : Boxable<Int> {
  fun get(): Int { return this.n }
}
fun take(b: Boxable<Int>): Int {
  return b.get()
}
fun main() {
  val x = IntBox(7)
  val n = take(x)
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("generic iface implements");
    assert!(
        checked
            .mono_interfaces
            .iter()
            .any(|(n, args)| n == "Boxable" && args == &[Ty::Int]),
        "expected mono Boxable<Int>, got {:?}",
        checked.mono_interfaces
    );
}

#[test]
fn generic_class_implements_mono() {
    // C9a: class Box<T> : Boxable<T>
    let src = r#"
package t
interface Boxable<T> {
  fun get(): T
}
class Box<T>(val v: T) : Boxable<T> {
  fun get(): T { return this.v }
}
fun take(b: Boxable<Int>): Int {
  return b.get()
}
fun main() {
  val x = Box(7)
  val n = take(x)
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("generic class implements");
    assert!(
        checked
            .mono_classes
            .iter()
            .any(|(n, args)| n == "Box" && args == &[Ty::Int]),
        "expected mono Box<Int>, got {:?}",
        checked.mono_classes
    );
    assert!(
        checked
            .mono_interfaces
            .iter()
            .any(|(n, args)| n == "Boxable" && args == &[Ty::Int]),
        "expected mono Boxable<Int> from class implements subst, got {:?}",
        checked.mono_interfaces
    );
}

#[test]
fn array_accepts_class_elem() {
    let src = r#"
package t
class Box(val x: Int) {}
fun main() {
  val a: Array<Box> = Array(1)
  a.set(0, Box(2))
  val b: Box = a.get(0)
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("class Array elem");
}

#[test]
fn generic_higher_order_function_typechecks() {
    let src = r#"
package t
fun map<T, R>(xs: Array<T>, f: (T) -> R): Array<R> {
  val out: Array<R> = Array(xs.len)
  var i: Int = 0
  while (i < xs.len) {
    out.set(i, f(xs.get(i)))
    i = i + 1
  }
  return out
}
fun filter<T>(xs: Array<T>, pred: (T) -> Bool): Array<T> {
  val out: Array<T> = Array(0)
  var i: Int = 0
  while (i < xs.len) {
    if (pred(xs.get(i))) { out.push(xs.get(i)) }
    i = i + 1
  }
  return out
}
fun fold<T, A>(xs: Array<T>, init: A, f: (A, T) -> A): A {
  var acc: A = init
  var i: Int = 0
  while (i < xs.len) {
    acc = f(acc, xs.get(i))
    i = i + 1
  }
  return acc
}
fun main() {
  val xs: Array<Int> = Array(2)
  xs.set(0, 2)
  xs.set(1, 3)
  val ys: Array<String> = map<Int, String>(xs, (x: Int) => x.toString())
  val zs: Array<Int> = filter<Int>(xs, (x: Int) => x > 2)
  val total: Int = fold<Int, Int>(xs, 0, (acc: Int, x: Int) => acc + x)
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("generic HOFs");
}

#[test]
fn generic_higher_order_function_accepts_generic_class_values() {
    let src = r#"
package t
class Box<T>(val value: T) {}
fun map<T, R>(xs: Array<T>, f: (T) -> R): Array<R> {
  val out: Array<R> = Array(xs.len)
  var i: Int = 0
  while (i < xs.len) {
    out.set(i, f(xs.get(i)))
    i = i + 1
  }
  return out
}
fun filter<T>(xs: Array<T>, pred: (T) -> Bool): Array<T> {
  val out: Array<T> = Array(0)
  var i: Int = 0
  while (i < xs.len) {
    if (pred(xs.get(i))) { out.push(xs.get(i)) }
    i = i + 1
  }
  return out
}
fun fold<T, A>(xs: Array<T>, init: A, f: (A, T) -> A): A {
  var acc: A = init
  var i: Int = 0
  while (i < xs.len) {
    acc = f(acc, xs.get(i))
    i = i + 1
  }
  return acc
}
fun main() {
  val xs: Array<Box<Int>> = Array(0)
  xs.push(Box<Int>(1))
  val ys: Array<Box<Int>> = map<Box<Int>, Box<Int>>(xs, (x: Box<Int>) => Box<Int>(x.value + 1))
  val zs: Array<Box<Int>> = filter<Box<Int>>(ys, (x: Box<Int>) => x.value > 1)
  val total: Box<Int> = fold<Box<Int>, Box<Int>>(zs, Box<Int>(0), (a: Box<Int>, x: Box<Int>) => Box<Int>(a.value + x.value))
}
"#;
    let file = parse_file(src).expect("parse generic class HOF");
    check_file(&file).expect("generic HOFs over generic class values");
}

#[test]
fn array_accepts_struct_elem() {
    let src = r#"
package t
struct Point(val x: Int) {}
fun main() {
  val a: Array<Point> = Array(1)
  a.set(0, Point(2))
  val p: Point = a.get(0)
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("struct Array elem");
}

#[test]
fn reject_struct_equality() {
    let src = r#"
package t
struct Point(val x: Int) {}
fun main() {
  val a: Point = Point(1)
  val b: Point = Point(1)
  if (a == b) {}
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("struct ==");
    assert!(
        err.primary().message.contains("struct") || err.primary().message.contains("compare"),
        "{}",
        err.primary().message
    );
}

#[test]
fn reject_enum_equality() {
    let src = r#"
package t
enum Color { case Red case Blue }
fun main() {
  val a: Color = Red()
  val b: Color = Red()
  if (a == b) {}
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("enum ==");
    assert!(
        err.primary().message.contains("enum") || err.primary().message.contains("compare"),
        "{}",
        err.primary().message
    );
}

#[test]
fn lambda_allows_var_class_capture_by_ref() {
    // C20a: mutable class captures are represented as by-ref captures.
    let src = r#"
package t
class Box(val n: Int) {}
fun main() {
  var b = Box(1)
  val f = () => b.n
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("var class capture should be allowed");
    assert!(checked.lambda_captures.values().any(|caps| {
        caps.iter()
            .any(|c| c.name == "b" && c.by_ref && matches!(c.ty, Ty::Class(_)))
    }));
}

#[test]
fn lambda_allows_var_array_capture_by_ref() {
    // C20a: mutable Array captures retain the view/reference contract.
    let src = r#"
package t
fun main() {
  var a: Array<Int> = Array(1)
  val f = () => a.len
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("var Array capture should be allowed");
    assert!(checked.lambda_captures.values().any(|caps| {
        caps.iter().any(|c| {
            c.name == "a"
                && c.by_ref
                && matches!(&c.ty, Ty::ClassApp { name, .. } if name == "Array")
        })
    }));
}

#[test]
fn lambda_allows_var_string_capture() {
    // C13f: outer `var` String is capturable via shared RC box.
    let src = r#"
package t
fun main(): String {
  var s = "hi"
  val f = () => s
  return f()
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("var String capture should be allowed");
    let has_str_cap = checked.lambda_captures.values().any(|caps| {
        caps.iter()
            .any(|c| c.name == "s" && c.by_ref && matches!(c.ty, Ty::String))
    });
    assert!(
        has_str_cap,
        "expected outer lambda to by-ref capture String `s`"
    );
}

#[test]
fn lambda_allows_fun_capture() {
    // C13e: outer `val` Fun is capturable (nested env retain/release in codegen).
    let src = r#"
package t
fun main(): Int {
  val inner: (Int) -> Int = (x: Int) => x + 1
  val outer = () => inner(2)
  return outer()
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("Fun capture should be allowed");
    let has_fun_cap = checked.lambda_captures.values().any(|caps| {
        caps.iter()
            .any(|c| c.name == "inner" && matches!(c.ty, Ty::Fun { .. }))
    });
    assert!(has_fun_cap, "expected outer lambda to capture Fun `inner`");
}

#[test]
fn lambda_allows_var_fun_capture_by_ref() {
    // C20a: mutable Fun captures carry the nested closure reference by ref.
    let src = r#"
package t
fun main() {
  var f: (Int) -> Int = (x: Int) => x
  val g = () => f(1)
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("var Fun capture should be allowed");
    assert!(checked.lambda_captures.values().any(|caps| {
        caps.iter()
            .any(|c| c.name == "f" && c.by_ref && matches!(c.ty, Ty::Fun { .. }))
    }));
}
