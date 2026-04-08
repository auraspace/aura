use aura_mir::{dump_mir, lower_program};
use aura_parser::parse_program;
use aura_typeck::typeck_program;

#[test]
fn lowers_try_catch_finally_into_cleanup_region() {
    let src = r#"
function f(): void {
  try {
    throw 1;
  } catch (e) {
    return;
  } finally {
    let done = 1;
  }
}
"#;

    let parsed = parse_program(src);
    assert!(parsed.errors.is_empty(), "{:#?}", parsed.errors);

    let (diags, typed) = typeck_program(src, &parsed.value);
    assert!(diags.is_empty(), "{diags:#?}");

    let mir = lower_program(src, &parsed.value, &typed);
    assert_eq!(mir.functions.len(), 1);
    assert_eq!(mir.functions[0].cleanup_regions.len(), 1);

    let rendered = dump_mir(&mir);
    assert!(rendered.contains("cleanup region"));
    assert!(rendered.contains("catch -> bb"));
    assert!(rendered.contains("finally -> bb"));
}
