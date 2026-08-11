#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let source = String::from_utf8_lossy(input);
    let _ = aura_package::parse_manifest_for_fuzz(&source);
});
