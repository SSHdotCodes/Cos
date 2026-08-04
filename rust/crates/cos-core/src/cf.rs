//! Small Core Foundation helpers for FFI dictionaries built from raw refs.

use core_foundation::base::TCFType;
use core_foundation::dictionary::CFDictionary;
use core_foundation_sys::base::{CFAllocatorRef, CFIndex, CFTypeRef};
use std::ffi::c_void;

unsafe extern "C" {
    fn CFDictionaryCreate(
        allocator: CFAllocatorRef,
        keys: *const *const c_void,
        values: *const *const c_void,
        num_values: CFIndex,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> core_foundation_sys::dictionary::CFDictionaryRef;

    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
}

/// Build a CFDictionary from raw key/value refs (retained by Core Foundation).
pub fn dictionary_from_raw_pairs(pairs: &[(CFTypeRef, CFTypeRef)]) -> CFDictionary {
    let keys: Vec<*const c_void> = pairs.iter().map(|(key, _)| *key as *const c_void).collect();
    let values: Vec<*const c_void> = pairs.iter().map(|(_, value)| *value as *const c_void).collect();
    unsafe {
        let reference = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            keys.len() as CFIndex,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        CFDictionary::wrap_under_create_rule(reference)
    }
}
