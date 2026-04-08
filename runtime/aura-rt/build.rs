fn main() {
    let mut build = cc::Build::new();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build.flag("-mmacosx-version-min=11.0");
    }

    build
        .file("c/exception.c")
        .flag_if_supported("-Wno-unused-parameter")
        .compile("aura_rt_exception");
}
