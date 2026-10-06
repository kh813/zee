//! Platform-specific Input Method Editor (IME) detection.

/// Returns `true` if the current input source is a CJK (Chinese, Japanese, Korean) IME.
pub fn is_cjk_ime_active() -> bool {
    #[cfg(target_os = "macos")]
    {
        detect_macos_ime()
    }

    #[cfg(target_os = "windows")]
    {
        detect_windows_ime()
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        detect_linux_ime()
    }
}

#[cfg(target_os = "macos")]
fn detect_macos_ime() -> bool {
    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn TISCopyCurrentKeyboardInputSource() -> *mut std::ffi::c_void;
        fn TISGetInputSourceProperty(
            source: *mut std::ffi::c_void,
            property_key: *const std::ffi::c_void,
        ) -> *const std::ffi::c_void;
        static kTISPropertyInputSourceID: *const std::ffi::c_void;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *const std::ffi::c_void);
        fn CFStringGetLength(the_string: *const std::ffi::c_void) -> isize;
        fn CFStringGetCString(
            the_string: *const std::ffi::c_void,
            buffer: *mut u8,
            buffer_size: isize,
            encoding: u32,
        ) -> bool;
    }

    unsafe {
        let source = TISCopyCurrentKeyboardInputSource();
        if source.is_null() {
            return false;
        }

        let id_ref = TISGetInputSourceProperty(source, kTISPropertyInputSourceID);
        if id_ref.is_null() {
            CFRelease(source);
            return false;
        }

        let len = CFStringGetLength(id_ref);
        let mut buf = vec![0u8; (len * 4 + 1) as usize];
        let ok = CFStringGetCString(id_ref, buf.as_mut_ptr(), buf.len() as isize, 0x08000100);
        CFRelease(source);

        if !ok {
            return false;
        }

        if let Ok(id_str) = std::ffi::CStr::from_ptr(buf.as_ptr() as *const i8).to_str() {
            let lower = id_str.to_lowercase();
            // Explicit Roman/Latin ASCII modes are not CJK active
            if lower.contains("roman") || lower.contains("keylayout.abc") || lower.contains("keylayout.us") {
                return false;
            }
            // Detect CJK input methods
            lower.contains("japanese")
                || lower.contains("kotoeri")
                || lower.contains("korean")
                || lower.contains("hangeul")
                || lower.contains("scim")
                || lower.contains("tcim")
                || lower.contains("pinyin")
                || lower.contains("zhuyin")
                || lower.contains("cangjie")
                || lower.contains("wubi")
                || lower.contains("inputmethod")
        } else {
            false
        }
    }
}

#[cfg(target_os = "windows")]
fn detect_windows_ime() -> bool {
    extern "system" {
        fn GetKeyboardLayout(id_thread: u32) -> *mut std::ffi::c_void;
    }
    unsafe {
        let layout = GetKeyboardLayout(0) as usize;
        let lang_id = (layout & 0xFFFF) as u16;
        // 0x0411: Japanese, 0x0412: Korean, 0x0804 / 0x0404: Chinese
        matches!(lang_id, 0x0411 | 0x0412 | 0x0804 | 0x0404 | 0x0c04 | 0x1004 | 0x1404)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn detect_linux_ime() -> bool {
    if let Ok(m) = std::env::var("GTK_IM_MODULE") {
        let lower = m.to_lowercase();
        if lower.contains("ibus") || lower.contains("fcitx") || lower.contains("uim") {
            return true;
        }
    }
    if let Ok(m) = std::env::var("QT_IM_MODULE") {
        let lower = m.to_lowercase();
        if lower.contains("ibus") || lower.contains("fcitx") || lower.contains("uim") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_cjk_ime_active_runs_without_panic() {
        // Must not crash or segfault on any platform
        let _ = is_cjk_ime_active();
    }
}
