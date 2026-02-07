use std::ffi::CString;
use std::os::raw::c_char;

/// Allocates a C string containing the keepbook-ffi version.
///
/// Call `keepbook_ffi_string_free` to free the returned pointer.
#[no_mangle]
pub extern "C" fn keepbook_ffi_version() -> *mut c_char {
    CString::new(env!("CARGO_PKG_VERSION"))
        .expect("version should be valid C string")
        .into_raw()
}

/// Frees a string allocated by this library.
#[no_mangle]
pub extern "C" fn keepbook_ffi_string_free(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(s));
    }
}

// Android entrypoint used by the Expo module's Kotlin code.
#[cfg(target_os = "android")]
mod android {
    use jni::objects::JClass;
    use jni::sys::jstring;
    use jni::JNIEnv;

    #[no_mangle]
    pub extern "system" fn Java_expo_modules_keepbooknative_KeepbookNativeRust_version(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        let s = env!("CARGO_PKG_VERSION");
        env.new_string(s)
            .expect("Couldn't create java string")
            .into_raw()
    }
}
