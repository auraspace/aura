use std::alloc::{alloc, Layout};
use std::ffi::CStr;
use std::os::raw::c_char;

/// Allocate memory dynamically for the Aura runtime.
///
/// In a fully complete system, this interfaces directly with `GcHeap`.
/// For the ABI stub, we use standard allocation.
#[no_mangle]
pub extern "C" fn aura_alloc(size: usize) -> *mut u8 {
    let layout = Layout::from_size_align(size, 8).unwrap();
    unsafe { alloc(layout) }
}

/// The Garbage Collector write barrier.
///
/// Called whenever `obj.field = val` occurs.
#[no_mangle]
pub extern "C" fn aura_write_barrier(obj: *mut u8, field_val: *mut u8) {
    // In a mature system, if `obj` is in the Old Generation and `field_val`
    // is in the Young Generation, we add `obj` to the GC remember set.
    // For now, this is a placeholder verifying the ABI boundary.
    let _ = obj;
    let _ = field_val;
}

#[no_mangle]
pub extern "C" fn print_num(val: i64) {
    println!("{}", val);
}

#[no_mangle]
pub extern "C" fn print_str(ptr: *const c_char) {
    if ptr.is_null() {
        println!("null");
    } else {
        let c_str = unsafe { CStr::from_ptr(ptr) };
        println!("{}", c_str.to_string_lossy());
    }
}
#[no_mangle]
pub extern "C" fn print_bool(val: i64) {
    if val != 0 {
        println!("true");
    } else {
        println!("false");
    }
}
