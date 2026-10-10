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

pub(crate) fn evaluate_input_source_cjk(
    input_mode: Option<&str>,
    bundle_id: Option<&str>,
    layout_name: Option<&str>,
) -> Option<bool> {
    // 1. Check Input Mode if available
    let mut mode_is_cjk = false;
    let mut mode_is_ascii = false;
    if let Some(mode) = input_mode {
        let lower = mode.to_lowercase();
        if lower.contains("roman") || lower.contains("ascii") {
            mode_is_ascii = true;
        } else if lower.contains("japanese")
            || lower.contains("korean")
            || lower.contains("chinese")
            || lower.contains("pinyin")
            || lower.contains("zhuyin")
            || lower.contains("hangul")
        {
            mode_is_cjk = true;
        }
    }

    if mode_is_cjk {
        return Some(true);
    }
    if mode_is_ascii {
        return Some(false);
    }

    // 2. Check Layout Name (e.g. "ABC", "US")
    if let Some(name) = layout_name {
        let lower = name.to_lowercase();
        if lower == "abc" || lower == "us" || lower.contains("roman") || lower.contains("ascii") {
            return Some(false);
        }
    }

    // 3. Check Bundle ID / Source ID
    if let Some(bundle) = bundle_id {
        let lower = bundle.to_lowercase();
        if lower.contains("keylayout.abc")
            || lower.contains("keylayout.us")
            || lower.contains("roman")
            || lower.ends_with(".romajityping")
        {
            return Some(false);
        }
        if lower.contains(".japanese")
            || lower.contains("korean")
            || lower.contains("hangeul")
            || lower.contains("pinyin")
            || lower.contains("zhuyin")
        {
            return Some(true);
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn detect_macos_ime() -> bool {
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFPreferencesAppSynchronize(application_id: *const std::ffi::c_void) -> u8;
        fn CFPreferencesCopyAppValue(
            key: *const std::ffi::c_void,
            application_id: *const std::ffi::c_void,
        ) -> *const std::ffi::c_void;
        fn CFGetTypeID(cf: *const std::ffi::c_void) -> usize;
        fn CFArrayGetTypeID() -> usize;
        fn CFArrayGetCount(the_array: *const std::ffi::c_void) -> isize;
        fn CFArrayGetValueAtIndex(the_array: *const std::ffi::c_void, idx: isize) -> *const std::ffi::c_void;
        fn CFDictionaryGetTypeID() -> usize;
        fn CFDictionaryGetValue(
            the_dict: *const std::ffi::c_void,
            key: *const std::ffi::c_void,
        ) -> *const std::ffi::c_void;
        fn CFStringCreateWithCString(
            alloc: *const std::ffi::c_void,
            c_str: *const std::ffi::c_char,
            encoding: u32,
        ) -> *const std::ffi::c_void;
        fn CFStringGetLength(the_string: *const std::ffi::c_void) -> isize;
        fn CFStringGetCString(
            the_string: *const std::ffi::c_void,
            buffer: *mut u8,
            buffer_size: isize,
            encoding: u32,
        ) -> bool;
        fn CFRelease(cf: *const std::ffi::c_void);
    }

    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn TISCopyCurrentKeyboardInputSource() -> *mut std::ffi::c_void;
        fn TISGetInputSourceProperty(
            source: *mut std::ffi::c_void,
            property_key: *const std::ffi::c_void,
        ) -> *const std::ffi::c_void;
        static kTISPropertyInputSourceID: *const std::ffi::c_void;
        static kTISPropertyInputModeID: *const std::ffi::c_void;
    }

    unsafe {
        let cfstring_to_string = |cf_str: *const std::ffi::c_void| -> Option<String> {
            if cf_str.is_null() {
                return None;
            }
            let len = CFStringGetLength(cf_str);
            let mut buf = vec![0u8; (len * 4 + 1) as usize];
            if CFStringGetCString(cf_str, buf.as_mut_ptr(), buf.len() as isize, 0x08000100) {
                std::ffi::CStr::from_ptr(buf.as_ptr() as *const i8)
                    .to_str()
                    .ok()
                    .map(|s| s.to_string())
            } else {
                None
            }
        };

        let make_cfstring = |s: &str| -> *const std::ffi::c_void {
            let c_str = std::ffi::CString::new(s).unwrap();
            CFStringCreateWithCString(std::ptr::null(), c_str.as_ptr(), 0x08000100)
        };

        // 1. Primary approach: Query AppleSelectedInputSources from com.apple.HIToolbox
        // This reflects live global macOS IME state for CLI processes without an NSApplication runloop
        let app_id = make_cfstring("com.apple.HIToolbox");
        let key_selected = make_cfstring("AppleSelectedInputSources");

        CFPreferencesAppSynchronize(app_id);
        let selected_val = CFPreferencesCopyAppValue(key_selected, app_id);

        if !selected_val.is_null() {
            if CFGetTypeID(selected_val) == CFArrayGetTypeID() {
                let count = CFArrayGetCount(selected_val);
                let key_mode = make_cfstring("Input Mode");
                let key_bundle = make_cfstring("Bundle ID");
                let key_name = make_cfstring("KeyboardLayout Name");

                let mut result = None;

                for i in 0..count {
                    let item = CFArrayGetValueAtIndex(selected_val, i);
                    if !item.is_null() && CFGetTypeID(item) == CFDictionaryGetTypeID() {
                        let mode_ref = CFDictionaryGetValue(item, key_mode);
                        let mode_str = cfstring_to_string(mode_ref);

                        let bundle_ref = CFDictionaryGetValue(item, key_bundle);
                        let bundle_str = cfstring_to_string(bundle_ref);

                        let name_ref = CFDictionaryGetValue(item, key_name);
                        let name_str = cfstring_to_string(name_ref);

                        if let Some(res) = evaluate_input_source_cjk(
                            mode_str.as_deref(),
                            bundle_str.as_deref(),
                            name_str.as_deref(),
                        ) {
                            result = Some(res);
                            if res {
                                break;
                            }
                        }
                    }
                }

                CFRelease(key_mode);
                CFRelease(key_bundle);
                CFRelease(key_name);
                CFRelease(selected_val);
                CFRelease(key_selected);
                CFRelease(app_id);

                if let Some(res) = result {
                    return res;
                }
            } else {
                CFRelease(selected_val);
                CFRelease(key_selected);
                CFRelease(app_id);
            }
        } else {
            CFRelease(key_selected);
            CFRelease(app_id);
        }

        // 2. Fallback to TISCopyCurrentKeyboardInputSource
        let source = TISCopyCurrentKeyboardInputSource();
        if source.is_null() {
            return false;
        }

        let mode_ref = TISGetInputSourceProperty(source, kTISPropertyInputModeID);
        let mode_str = cfstring_to_string(mode_ref);

        let id_ref = TISGetInputSourceProperty(source, kTISPropertyInputSourceID);
        let id_str = cfstring_to_string(id_ref);

        CFRelease(source);

        evaluate_input_source_cjk(mode_str.as_deref(), id_str.as_deref(), None).unwrap_or(false)
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
        let _ = is_cjk_ime_active();
    }

    #[test]
    fn test_evaluate_input_source_cjk_cases() {
        // macOS Japanese Kana mode
        assert_eq!(
            evaluate_input_source_cjk(
                Some("com.apple.inputmethod.Japanese"),
                Some("com.apple.inputmethod.Kotoeri.RomajiTyping"),
                None
            ),
            Some(true)
        );

        // macOS Japanese Eisu mode (RomajiTyping without Japanese mode)
        assert_eq!(
            evaluate_input_source_cjk(
                None,
                Some("com.apple.inputmethod.Kotoeri.RomajiTyping"),
                None
            ),
            Some(false)
        );

        // Standard ABC keyboard layout
        assert_eq!(
            evaluate_input_source_cjk(
                None,
                Some("com.apple.keylayout.ABC"),
                Some("ABC")
            ),
            Some(false)
        );

        // Google Japanese IME - Kana
        assert_eq!(
            evaluate_input_source_cjk(
                Some("com.apple.inputmethod.Japanese"),
                Some("com.google.inputmethod.Japanese.base"),
                None
            ),
            Some(true)
        );

        // Google Japanese IME - Eisu
        assert_eq!(
            evaluate_input_source_cjk(
                Some("com.apple.inputmethod.Roman"),
                Some("com.google.inputmethod.Japanese.Roman"),
                None
            ),
            Some(false)
        );

        // ATOK - Kana
        assert_eq!(
            evaluate_input_source_cjk(
                Some("com.apple.inputmethod.Japanese"),
                Some("com.justsystems.inputmethod.atok.Japanese"),
                None
            ),
            Some(true)
        );

        // ATOK - Eisu
        assert_eq!(
            evaluate_input_source_cjk(
                Some("com.apple.inputmethod.Roman"),
                Some("com.justsystems.inputmethod.atok.Roman"),
                None
            ),
            Some(false)
        );
    }
}
