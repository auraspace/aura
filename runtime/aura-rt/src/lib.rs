use std::alloc::{alloc, Layout};
use std::ptr;

#[repr(C)]
pub struct AuraObject {
    pub vtable: *mut (),
    pub ref_count: usize,
}

#[repr(C)]
pub struct AuraString {
    pub header: AuraObject,
    pub len: usize,
    pub data: *const u8,
}

#[no_mangle]
pub unsafe extern "C" fn aura_alloc(size: usize, align: usize) -> *mut u8 {
    let layout = Layout::from_size_align_unchecked(size, align);
    let ptr = alloc(layout);
    if ptr.is_null() {
        aura_panic(b"out of memory\0".as_ptr(), 13);
    }
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn aura_retain(obj: *mut AuraObject) {
    if !obj.is_null() {
        (*obj).ref_count += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn aura_release(obj: *mut AuraObject) {
    if !obj.is_null() {
        (*obj).ref_count -= 1;
        if (*obj).ref_count == 0 {
            // TODO: Free the object and its fields based on vtable/type info.
            // For MVP strings/primitives, we just free the memory block.
            // But we need the layout to free it correctly.
            // For now, let's just avoid leaking the string data if possible.
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn aura_string_new_utf8(ptr: *const u8, len: usize) -> *mut AuraString {
    let size = std::mem::size_of::<AuraString>();
    let align = std::mem::align_of::<AuraString>();
    let s_ptr = aura_alloc(size, align) as *mut AuraString;

    // Initialize header
    (*s_ptr).header.vtable = ptr::null_mut();
    (*s_ptr).header.ref_count = 1;

    // Copy string data
    let data_ptr = aura_alloc(len, 1);
    ptr::copy_nonoverlapping(ptr, data_ptr, len);

    (*s_ptr).len = len;
    (*s_ptr).data = data_ptr;

    s_ptr
}

#[no_mangle]
pub unsafe extern "C" fn aura_println(str: *mut AuraString) {
    if !str.is_null() {
        let s = std::slice::from_raw_parts((*str).data, (*str).len);
        if let Ok(utf8) = std::str::from_utf8(s) {
            println!("{}", utf8);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn aura_panic(msg_ptr: *const u8, msg_len: usize) -> ! {
    let s = std::slice::from_raw_parts(msg_ptr, msg_len);
    let msg = std::str::from_utf8(s).unwrap_or("unknown panic");
    eprintln!("aura panic: {}", msg);
    std::process::abort();
}
