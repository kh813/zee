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

/// Returns `true` if the current process is running inside an SSH session.
pub fn is_ssh_session() -> bool {
    std::env::var("SSH_CONNECTION").is_ok()
        || std::env::var("SSH_CLIENT").is_ok()
        || std::env::var("SSH_TTY").is_ok()
}

/// Returns `true` if the character is typically inputted via CJK IME (wide characters, kana, kanji, etc.).
pub fn is_cjk_char(c: char) -> bool {
    unicode_width::UnicodeWidthChar::width(c).unwrap_or(0) >= 2
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

/// Evaluates Linux IME status based on output from `fcitx5-remote`, `fcitx-remote`, or `ibus engine`.
#[allow(dead_code)]
pub(crate) fn evaluate_linux_ime_status(raw: &str) -> Option<bool> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. Fcitx5 / Fcitx remote output:
    // 0 = close / unavailable
    // 1 = inactive (direct input / English)
    // 2 = active (IME on / Japanese, etc.)
    match trimmed {
        "2" => return Some(true),
        "1" | "0" => return Some(false),
        _ => {}
    }

    // 2. IBus engine output:
    // "xkb:us::eng", "xkb:jp::jpn" etc. -> direct keyboard layout (IME off)
    // "mozc-jp", "anthy", "kkc", "libpinyin", "hangul" etc. -> IME engine active
    let lower = trimmed.to_lowercase();
    if lower.starts_with("xkb:") {
        return Some(false);
    }

    if lower.contains("mozc")
        || lower.contains("anthy")
        || lower.contains("kkc")
        || lower.contains("skk")
        || lower.contains("pinyin")
        || lower.contains("hangul")
        || lower.contains("chewing")
        || lower.contains("cangjie")
        || lower.contains("rime")
        || lower.contains("bogo")
        || lower.contains("japanese")
        || lower.contains("korean")
        || lower.contains("chinese")
    {
        return Some(true);
    }

    None
}

/// Evaluates Windows IME status given the open status and keyboard layout language ID.
#[allow(dead_code)]
pub(crate) fn evaluate_windows_ime_status(is_open: bool, lang_id: u16) -> bool {
    // 0x0411: Japanese
    // 0x0412: Korean
    // 0x0804, 0x0404, 0x0c04, 0x1004, 0x1404: Chinese variants
    let is_cjk_lang = matches!(
        lang_id,
        0x0411 | 0x0412 | 0x0804 | 0x0404 | 0x0c04 | 0x1004 | 0x1404
    );

    is_cjk_lang && is_open
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
        fn GetForegroundWindow() -> *mut std::ffi::c_void;
        fn ImmGetDefaultIMEWnd(hwnd: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn SendMessageA(hwnd: *mut std::ffi::c_void, msg: u32, wparam: usize, lparam: isize) -> isize;
    }
    unsafe {
        let layout = GetKeyboardLayout(0) as usize;
        let lang_id = (layout & 0xFFFF) as u16;

        let hwnd = GetForegroundWindow();
        let is_open = if !hwnd.is_null() {
            let ime_hwnd = ImmGetDefaultIMEWnd(hwnd);
            if !ime_hwnd.is_null() {
                // WM_IME_CONTROL = 0x0283, IMC_GETOPENSTATUS = 0x0005
                SendMessageA(ime_hwnd, 0x0283, 0x0005, 0) != 0
            } else {
                false
            }
        } else {
            false
        };

        evaluate_windows_ime_status(is_open, lang_id)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn detect_linux_ime() -> bool {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::Instant;

    static LAST_CHECK_MS: AtomicU64 = AtomicU64::new(0);
    static LAST_RESULT: AtomicBool = AtomicBool::new(false);
    static START_INSTANT: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

    let start = START_INSTANT.get_or_init(Instant::now);
    let now_ms = start.elapsed().as_millis() as u64;
    let last = LAST_CHECK_MS.load(Ordering::Relaxed);

    // Throttle queries to once every 100ms to avoid excessive process spawns in TUI event loop
    if now_ms.saturating_sub(last) < 100 {
        return LAST_RESULT.load(Ordering::Relaxed);
    }

    let result = query_linux_ime();
    LAST_RESULT.store(result, Ordering::Relaxed);
    LAST_CHECK_MS.store(now_ms, Ordering::Relaxed);
    result
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn query_linux_ime() -> bool {
    // 1. Try fcitx5-remote
    if let Ok(output) = std::process::Command::new("fcitx5-remote").output() {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout);
            if let Some(status) = evaluate_linux_ime_status(&s) {
                return status;
            }
        }
    }

    // 2. Try fcitx-remote
    if let Ok(output) = std::process::Command::new("fcitx-remote").output() {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout);
            if let Some(status) = evaluate_linux_ime_status(&s) {
                return status;
            }
        }
    }

    // 3. Try ibus engine
    if let Ok(output) = std::process::Command::new("ibus").arg("engine").output() {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout);
            if let Some(status) = evaluate_linux_ime_status(&s) {
                return status;
            }
        }
    }

    // Fallback: If no dynamic tool answered, default to false (ASCII) to prevent
    // false-positive Japanese mode.
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

    #[test]
    fn test_evaluate_linux_ime_status_cases() {
        // Fcitx 5 / Fcitx 4
        assert_eq!(evaluate_linux_ime_status("2"), Some(true));
        assert_eq!(evaluate_linux_ime_status("2\n"), Some(true));
        assert_eq!(evaluate_linux_ime_status("2\r\n"), Some(true));
        assert_eq!(evaluate_linux_ime_status("1"), Some(false));
        assert_eq!(evaluate_linux_ime_status("1\n"), Some(false));
        assert_eq!(evaluate_linux_ime_status("0"), Some(false));

        // IBus CJK engines
        assert_eq!(evaluate_linux_ime_status("mozc-jp"), Some(true));
        assert_eq!(evaluate_linux_ime_status("anthy"), Some(true));
        assert_eq!(evaluate_linux_ime_status("kkc"), Some(true));
        assert_eq!(evaluate_linux_ime_status("skk"), Some(true));
        assert_eq!(evaluate_linux_ime_status("libpinyin"), Some(true));
        assert_eq!(evaluate_linux_ime_status("hangul"), Some(true));
        assert_eq!(evaluate_linux_ime_status("rime"), Some(true));

        // IBus direct keyboard layouts (IME off / English)
        assert_eq!(evaluate_linux_ime_status("xkb:us::eng"), Some(false));
        assert_eq!(evaluate_linux_ime_status("xkb:jp::jpn"), Some(false));

        // Empty or unknown
        assert_eq!(evaluate_linux_ime_status(""), None);
        assert_eq!(evaluate_linux_ime_status("   \n"), None);
        assert_eq!(evaluate_linux_ime_status("unknown_engine"), None);
    }

    #[test]
    fn test_evaluate_windows_ime_status_cases() {
        // Japanese (0x0411)
        assert!(evaluate_windows_ime_status(true, 0x0411));
        assert!(!evaluate_windows_ime_status(false, 0x0411));

        // Korean (0x0412)
        assert!(evaluate_windows_ime_status(true, 0x0412));
        assert!(!evaluate_windows_ime_status(false, 0x0412));

        // Chinese Simplified (0x0804) & Traditional (0x0404)
        assert!(evaluate_windows_ime_status(true, 0x0804));
        assert!(!evaluate_windows_ime_status(false, 0x0804));
        assert!(evaluate_windows_ime_status(true, 0x0404));
        assert!(!evaluate_windows_ime_status(false, 0x0404));

        // Non-CJK: US English (0x0409)
        assert!(!evaluate_windows_ime_status(true, 0x0409));
        assert!(!evaluate_windows_ime_status(false, 0x0409));

        // Non-CJK: German (0x0407)
        assert!(!evaluate_windows_ime_status(true, 0x0407));
        assert!(!evaluate_windows_ime_status(false, 0x0407));
    }

    #[test]
    fn test_is_cjk_char_cases() {
        assert!(is_cjk_char('あ'));
        assert!(is_cjk_char('ア'));
        assert!(is_cjk_char('漢'));
        assert!(is_cjk_char('字'));
        assert!(is_cjk_char('、'));
        assert!(is_cjk_char('。'));
        assert!(is_cjk_char('　')); // Full-width space
        assert!(is_cjk_char('Ａ')); // Full-width Latin
        assert!(is_cjk_char('한')); // Hangul

        assert!(!is_cjk_char('a'));
        assert!(!is_cjk_char('Z'));
        assert!(!is_cjk_char('0'));
        assert!(!is_cjk_char(' '));
        assert!(!is_cjk_char(':'));
        assert!(!is_cjk_char('\n'));
    }

    #[test]
    fn test_is_ssh_session_does_not_panic() {
        let _ = is_ssh_session();
    }
}
