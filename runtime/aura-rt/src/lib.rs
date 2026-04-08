use std::alloc::{alloc, Layout};
use std::cell::Cell;
use std::ffi::c_void;
use std::os::raw::c_int;
use std::ptr;

#[derive(Debug)]
#[repr(C)]
pub struct AuraObject {
    pub vtable: *mut (),
    pub ref_count: usize,
}

#[derive(Debug)]
#[repr(C)]
pub struct AuraJmpBuf {
    pub storage: [u32; 48],
}

#[derive(Debug)]
#[repr(C)]
pub struct AuraHandlerFrame {
    pub prev: *mut AuraHandlerFrame,
    pub catch_entry: *mut c_void,
    pub cleanup_stack: *mut c_void,
    pub jump_buf: AuraJmpBuf,
}

#[derive(Debug)]
#[repr(C)]
pub struct AuraString {
    pub header: AuraObject,
    pub len: usize,
    pub data: *const u8,
}

thread_local! {
    static CURRENT_HANDLER_FRAME: Cell<*mut AuraHandlerFrame> = Cell::new(ptr::null_mut());
    static CURRENT_EXCEPTION: Cell<*mut AuraObject> = Cell::new(ptr::null_mut());
}

fn current_handler_frame() -> *mut AuraHandlerFrame {
    CURRENT_HANDLER_FRAME.with(Cell::get)
}

fn set_current_handler_frame(frame: *mut AuraHandlerFrame) {
    CURRENT_HANDLER_FRAME.with(|current| current.set(frame));
}

fn current_exception() -> *mut AuraObject {
    CURRENT_EXCEPTION.with(Cell::get)
}

fn set_current_exception(exception: *mut AuraObject) {
    CURRENT_EXCEPTION.with(|current| current.set(exception));
}

fn clear_current_exception() {
    set_current_exception(ptr::null_mut());
}

extern "C" {
    fn aura_runtime_try_throw(frame: *mut AuraHandlerFrame, exception: *mut AuraObject) -> c_int;
    fn aura_runtime_longjmp(env: *mut c_void, value: c_int) -> !;
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
pub unsafe extern "C" fn aura_try_begin(frame: *mut AuraHandlerFrame) {
    if frame.is_null() {
        return;
    }

    (*frame).prev = current_handler_frame();
    set_current_handler_frame(frame);
}

#[no_mangle]
pub unsafe extern "C" fn aura_try_end(frame: *mut AuraHandlerFrame) {
    if frame.is_null() {
        return;
    }

    CURRENT_HANDLER_FRAME.with(|current| {
        let current_frame = current.get();
        if current_frame == frame {
            current.set((*frame).prev);
            (*frame).prev = ptr::null_mut();
            return;
        }

        let mut cursor = current_frame;
        while !cursor.is_null() {
            if (*cursor).prev == frame {
                (*cursor).prev = (*frame).prev;
                (*frame).prev = ptr::null_mut();
                return;
            }
            cursor = (*cursor).prev;
        }
    });
}

#[no_mangle]
pub unsafe extern "C" fn aura_current_exception() -> *mut AuraObject {
    current_exception()
}

#[no_mangle]
pub unsafe extern "C" fn aura_throw(exception: *mut AuraObject) -> ! {
    set_current_exception(exception);

    let frame = current_handler_frame();
    if frame.is_null() {
        aura_panic(b"uncaught exception\0".as_ptr(), 18);
    }

    aura_runtime_longjmp(
        (&mut (*frame).jump_buf) as *mut AuraJmpBuf as *mut c_void,
        1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::MaybeUninit;

    #[test]
    fn try_begin_and_end_manage_nested_frames() {
        let mut outer = AuraHandlerFrame {
            prev: ptr::null_mut(),
            catch_entry: ptr::null_mut(),
            cleanup_stack: ptr::null_mut(),
            jump_buf: AuraJmpBuf { storage: [0; 48] },
        };
        let mut inner = AuraHandlerFrame {
            prev: ptr::null_mut(),
            catch_entry: ptr::null_mut(),
            cleanup_stack: ptr::null_mut(),
            jump_buf: AuraJmpBuf { storage: [0; 48] },
        };
        let outer_ptr: *mut AuraHandlerFrame = &mut outer;
        let inner_ptr: *mut AuraHandlerFrame = &mut inner;

        unsafe {
            aura_try_begin(&mut outer);
            assert_eq!(current_handler_frame(), outer_ptr);
            assert_eq!(outer.prev, ptr::null_mut());

            aura_try_begin(&mut inner);
            assert_eq!(current_handler_frame(), inner_ptr);
            assert_eq!(inner.prev, outer_ptr);

            aura_try_end(&mut inner);
            assert_eq!(current_handler_frame(), outer_ptr);
            assert_eq!(inner.prev, ptr::null_mut());

            aura_try_end(&mut outer);
            assert_eq!(current_handler_frame(), ptr::null_mut());
            assert_eq!(outer.prev, ptr::null_mut());
        }
    }

    #[test]
    fn current_exception_storage_round_trips() {
        let mut object = AuraObject {
            vtable: ptr::null_mut(),
            ref_count: 1,
        };
        let object_ptr: *mut AuraObject = &mut object;

        set_current_exception(&mut object);
        assert_eq!(unsafe { aura_current_exception() }, object_ptr);

        clear_current_exception();
        assert_eq!(unsafe { aura_current_exception() }, ptr::null_mut());
    }

    #[test]
    fn throw_jumps_back_to_active_handler() {
        let mut frame = MaybeUninit::<AuraHandlerFrame>::zeroed();
        let mut object = AuraObject {
            vtable: ptr::null_mut(),
            ref_count: 1,
        };
        let object_ptr: *mut AuraObject = &mut object;

        unsafe {
            let jump = aura_runtime_try_throw(frame.as_mut_ptr(), object_ptr);
            assert_eq!(jump, 1);
            assert_eq!(aura_current_exception(), object_ptr);
        }
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
pub unsafe extern "C" fn aura_i32_to_string(val: i32) -> *mut AuraString {
    let s = val.to_string();
    aura_string_new_utf8(s.as_ptr(), s.len())
}

#[no_mangle]
pub unsafe extern "C" fn aura_i64_to_string(val: i64) -> *mut AuraString {
    let s = val.to_string();
    aura_string_new_utf8(s.as_ptr(), s.len())
}

#[no_mangle]
pub unsafe extern "C" fn aura_f32_to_string(val: f32) -> *mut AuraString {
    let s = val.to_string();
    aura_string_new_utf8(s.as_ptr(), s.len())
}

#[no_mangle]
pub unsafe extern "C" fn aura_f64_to_string(val: f64) -> *mut AuraString {
    let s = val.to_string();
    aura_string_new_utf8(s.as_ptr(), s.len())
}

#[no_mangle]
pub unsafe extern "C" fn aura_bool_to_string(val: bool) -> *mut AuraString {
    let s = val.to_string();
    aura_string_new_utf8(s.as_ptr(), s.len())
}

#[no_mangle]
pub unsafe extern "C" fn aura_panic(msg_ptr: *const u8, msg_len: usize) -> ! {
    let s = std::slice::from_raw_parts(msg_ptr, msg_len);
    let msg = std::str::from_utf8(s).unwrap_or("unknown panic");
    eprintln!("aura panic: {}", msg);
    std::process::abort();
}
