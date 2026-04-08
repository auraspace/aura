use aura_mir::{dump_mir, lower_program, CleanupReason};
use aura_parser::parse_program;
use aura_typeck::typeck_program;

#[test]
fn lowers_try_catch_finally_into_cleanup_region() {
    let src = r#"
function normal(): void {
  try {
    let done = 1;
  } finally {
    let cleanup = 1;
  }
}

function returns(): void {
  try {
    return;
  } finally {
    let cleanup = 1;
  }
}

function throws(): void {
  try {
    throw 1;
  } finally {
    let cleanup = 1;
  }
}
"#;

    let parsed = parse_program(src);
    assert!(parsed.errors.is_empty(), "{:#?}", parsed.errors);

    let (diags, typed) = typeck_program(src, &parsed.value);
    assert!(diags.is_empty(), "{diags:#?}");

    let mir = lower_program(src, &parsed.value, &typed);
    assert_eq!(mir.functions.len(), 3);
    for function in &mir.functions {
        assert_eq!(function.cleanup_regions.len(), 1, "{}", function.name);
        let region = &function.cleanup_regions[0];
        assert!(
            region
                .edges
                .iter()
                .any(|edge| edge.reason == CleanupReason::Normal),
            "{}",
            function.name
        );
        assert!(
            region
                .edges
                .iter()
                .any(|edge| edge.reason == CleanupReason::Return),
            "{}",
            function.name
        );
        assert!(
            region
                .edges
                .iter()
                .any(|edge| edge.reason == CleanupReason::Throw),
            "{}",
            function.name
        );
    }

    let rendered = dump_mir(&mir);
    assert!(rendered.contains("cleanup region"));
    assert!(rendered.contains("finally -> bb"));
    assert!(rendered.contains("edge bb"));
    assert!(rendered.contains("(Return)"));
    assert!(rendered.contains("(Throw)"));
}
