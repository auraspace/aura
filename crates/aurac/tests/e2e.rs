use aura_test_harness::aura_test;

aura_test!(hello, "tests/fixtures/e2e/hello/hello.aura");

// Control flow tests
aura_test!(
    while_loop,
    "tests/fixtures/e2e/control_flow/while_loop.aura"
);
aura_test!(
    if_else_logic,
    "tests/fixtures/e2e/control_flow/if_else_logic.aura"
);

// Primitive tests
aura_test!(math, "tests/fixtures/e2e/primitives/math.aura");
aura_test!(strings, "tests/fixtures/e2e/primitives/strings.aura");
aura_test!(booleans, "tests/fixtures/e2e/primitives/booleans.aura");

// Module tests
aura_test!(modules, "tests/fixtures/e2e/modules/main.aura");

// OOP tests
aura_test!(oop_fields, "tests/fixtures/e2e/oop/oop_fields.aura");
aura_test!(oop_methods, "tests/fixtures/e2e/oop/oop_methods.aura");
aura_test!(
    oop_inheritance,
    "tests/fixtures/e2e/oop/oop_inheritance.aura"
);
aura_test!(oop_interfaces, "tests/fixtures/e2e/oop/oop_interfaces.aura");

// Exception tests
aura_test!(
    exc_finally,
    "tests/fixtures/e2e/exceptions/exc_finally.aura"
);
aura_test!(exc_caught, "tests/fixtures/e2e/exceptions/exc_caught.aura");
aura_test!(exc_nested, "tests/fixtures/e2e/exceptions/exc_nested.aura");
aura_test!(
    exc_catch_return,
    "tests/fixtures/e2e/exceptions/exc_catch_return.aura"
);
aura_test!(
    exc_uncaught,
    "tests/fixtures/e2e/exceptions/exc_uncaught.aura"
);
