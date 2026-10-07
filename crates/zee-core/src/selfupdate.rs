use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GITHUB_REPO: &str = "kh813/zee";

/// Open a URL in default browser.
pub fn open_url(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(url).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(url).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd").args(["/c", "start", "", url]).spawn()?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppType {
    Gui,
    Cli,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GithubReleaseResponse {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub version: String,
    pub html_url: String,
    pub asset_name: Option<String>,
    pub asset_url: Option<String>,
    pub asset_size: Option<u64>,
    pub body: Option<String>,
}

/// Query GitHub Releases for the latest published release.
pub fn check_latest(app_type: AppType) -> Result<ReleaseInfo> {
    let api_url = format!("https://api.github.com/repos/{}/releases/latest", GITHUB_REPO);
    
    let resp = ureq::get(&api_url)
        .set("User-Agent", &format!("zee-updater/{}", CURRENT_VERSION))
        .set("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .context("Failed to connect to GitHub Releases API")?;

    if resp.status() != 200 {
        return Err(anyhow!("GitHub API returned HTTP status {}", resp.status()));
    }

    let payload: GithubReleaseResponse = resp.into_json()
        .context("Failed to parse GitHub release JSON")?;

    let version = payload.tag_name.trim_start_matches('v').to_string();
    let matching_asset = find_matching_asset(&payload.assets, app_type);

    Ok(ReleaseInfo {
        tag_name: payload.tag_name,
        version,
        html_url: payload.html_url,
        asset_name: matching_asset.map(|a| a.name.clone()),
        asset_url: matching_asset.map(|a| a.browser_download_url.clone()),
        asset_size: matching_asset.map(|a| a.size),
        body: payload.body,
    })
}

/// Find matching release asset for the current OS, CPU architecture, and application type.
pub fn find_matching_asset(assets: &[ReleaseAsset], app_type: AppType) -> Option<&ReleaseAsset> {
    find_matching_asset_for_platform(assets, app_type, std::env::consts::OS, std::env::consts::ARCH)
}

/// Find matching release asset for a specific target OS and architecture.
pub fn find_matching_asset_for_platform<'a>(
    assets: &'a [ReleaseAsset],
    app_type: AppType,
    os: &str,
    arch: &str,
) -> Option<&'a ReleaseAsset> {
    let matches_app_type = |name: &str| match app_type {
        AppType::Gui => name.starts_with("zee-gui-") || name.starts_with("zeeg-"),
        AppType::Cli => {
            (name.starts_with("zee-cli-") || name.starts_with("zee-"))
                && !name.starts_with("zee-gui-")
                && !name.starts_with("zeeg-")
        }
    };

    let is_arm64 = arch == "aarch64" || arch == "arm64";
    let matches_arch = |name: &str| {
        if is_arm64 {
            name.contains("arm64") || name.contains("aarch64")
        } else {
            name.contains("x64") || name.contains("x86_64")
        }
    };

    for asset in assets {
        let name = asset.name.to_lowercase();
        let is_archive = name.ends_with(".zip") || name.ends_with(".tar.gz");

        if !matches_app_type(&name) || !is_archive || !matches_arch(&name) {
            continue;
        }

        if (os == "macos" && name.contains("macos"))
            || (os == "linux" && name.contains("linux"))
            || (os == "windows" && name.contains("windows"))
        {
            return Some(asset);
        }
    }

    None
}

/// Compare two semantic version strings (e.g. "0.1.0" and "0.1.1").
pub fn is_newer(current: &str, latest: &str) -> bool {
    let c_parts = parse_version_numbers(current);
    let l_parts = parse_version_numbers(latest);

    match (c_parts, l_parts) {
        (Some(c), Some(l)) => {
            let max_len = c.len().max(l.len());
            for i in 0..max_len {
                let cv = c.get(i).copied().unwrap_or(0);
                let lv = l.get(i).copied().unwrap_or(0);
                if lv != cv {
                    return lv > cv;
                }
            }
            false
        }
        _ => latest != current && !latest.is_empty(),
    }
}

fn parse_version_numbers(v: &str) -> Option<Vec<u64>> {
    let cleaned = v.trim().trim_start_matches('v');
    let parts: Result<Vec<u64>, _> = cleaned.split('.').map(|s| s.parse::<u64>()).collect();
    parts.ok()
}

/// Download a file from URL to memory.
pub fn download_file(url: &str) -> Result<Vec<u8>> {
    let resp = ureq::get(url)
        .set("User-Agent", &format!("zee-updater/{}", CURRENT_VERSION))
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .context("Failed to download update package")?;

    if resp.status() != 200 {
        return Err(anyhow!("Failed to download update: HTTP status {}", resp.status()));
    }

    let mut reader = resp.into_reader();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).context("Failed to read update data")?;
    Ok(bytes)
}

/// Apply update from downloaded archive in place.
pub fn apply_update(asset_url: &str, app_type: AppType) -> Result<()> {
    let archive_bytes = download_file(asset_url)?;
    
    let current_exe = std::env::current_exe()
        .context("Could not determine currently running executable path")?;
    let current_exe = current_exe.canonicalize().unwrap_or(current_exe);

    #[cfg(target_os = "macos")]
    {
        if app_type == AppType::Gui {
            return apply_macos_gui_bundle(&archive_bytes, &current_exe);
        } else {
            return apply_unix_binary(&archive_bytes, &current_exe, "zee", false);
        }
    }

    #[cfg(target_os = "linux")]
    {
        let bin_name = if app_type == AppType::Gui { "zeeg" } else { "zee" };
        let is_gui = app_type == AppType::Gui;
        return apply_unix_binary(&archive_bytes, &current_exe, bin_name, is_gui);
    }

    #[cfg(target_os = "windows")]
    {
        let bin_name = if app_type == AppType::Gui { "zeeg.exe" } else { "zee.exe" };
        let is_gui = app_type == AppType::Gui;
        return apply_windows_binary(&archive_bytes, &current_exe, bin_name, is_gui);
    }

    #[allow(unreachable_code)]
    Err(anyhow!("Self-update is not supported on this platform"))
}

#[cfg(target_os = "macos")]
pub fn untranslocate_path(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    if !path_str.contains("AppTranslocation") {
        return path.to_path_buf();
    }

    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let path_bytes = path.as_os_str().as_bytes();
    let Ok(c_path) = CString::new(path_bytes) else {
        return path.to_path_buf();
    };

    type CFTypeRef = *const std::ffi::c_void;
    type CFURLRef = *const std::ffi::c_void;
    type CFErrorRef = *mut std::ffi::c_void;
    type Boolean = libc::c_uchar;
    type CFIndex = libc::c_long;

    type FnCFURLCreateFromFileSystemRepresentation = unsafe extern "C" fn(
        allocator: CFTypeRef,
        buffer: *const u8,
        buf_len: CFIndex,
        is_directory: Boolean,
    ) -> CFURLRef;

    type FnCFURLGetFileSystemRepresentation = unsafe extern "C" fn(
        url: CFURLRef,
        resolve_against_base: Boolean,
        buffer: *mut u8,
        max_buf_len: CFIndex,
    ) -> Boolean;

    type FnCFRelease = unsafe extern "C" fn(cf: CFTypeRef);

    type FnSecTranslocateIsTranslocatedURL = unsafe extern "C" fn(
        url: CFURLRef,
        is_translocated: *mut Boolean,
        error: *mut CFErrorRef,
    ) -> Boolean;

    type FnSecTranslocateCreateOriginalPathForURL = unsafe extern "C" fn(
        translocated_url: CFURLRef,
        error: *mut CFErrorRef,
    ) -> CFURLRef;

    unsafe {
        let sec_lib = libc::dlopen(
            c"/System/Library/Frameworks/Security.framework/Security".as_ptr(),
            libc::RTLD_LAZY,
        );
        let cf_lib = libc::dlopen(
            c"/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation".as_ptr(),
            libc::RTLD_LAZY,
        );

        if sec_lib.is_null() || cf_lib.is_null() {
            if !sec_lib.is_null() { libc::dlclose(sec_lib); }
            if !cf_lib.is_null() { libc::dlclose(cf_lib); }
            return path.to_path_buf();
        }

        let create_url_sym = libc::dlsym(cf_lib, c"CFURLCreateFromFileSystemRepresentation".as_ptr());
        let get_fs_sym = libc::dlsym(cf_lib, c"CFURLGetFileSystemRepresentation".as_ptr());
        let release_sym = libc::dlsym(cf_lib, c"CFRelease".as_ptr());
        let is_trans_sym = libc::dlsym(sec_lib, c"SecTranslocateIsTranslocatedURL".as_ptr());
        let create_orig_sym = libc::dlsym(sec_lib, c"SecTranslocateCreateOriginalPathForURL".as_ptr());

        if create_url_sym.is_null() || get_fs_sym.is_null() || release_sym.is_null()
            || is_trans_sym.is_null() || create_orig_sym.is_null()
        {
            libc::dlclose(sec_lib);
            libc::dlclose(cf_lib);
            return path.to_path_buf();
        }

        let cf_create_url: FnCFURLCreateFromFileSystemRepresentation = std::mem::transmute(create_url_sym);
        let cf_get_fs: FnCFURLGetFileSystemRepresentation = std::mem::transmute(get_fs_sym);
        let cf_release: FnCFRelease = std::mem::transmute(release_sym);
        let sec_is_trans: FnSecTranslocateIsTranslocatedURL = std::mem::transmute(is_trans_sym);
        let sec_create_orig: FnSecTranslocateCreateOriginalPathForURL = std::mem::transmute(create_orig_sym);

        let url = cf_create_url(
            std::ptr::null(),
            c_path.as_ptr() as *const u8,
            path_bytes.len() as CFIndex,
            1, // isDirectory: true
        );
        if url.is_null() {
            libc::dlclose(sec_lib);
            libc::dlclose(cf_lib);
            return path.to_path_buf();
        }

        let mut is_trans: Boolean = 0;
        let mut err: CFErrorRef = std::ptr::null_mut();
        let _ = sec_is_trans(url, &mut is_trans, &mut err);

        let mut resolved_path = None;
        if is_trans != 0 {
            let orig_url = sec_create_orig(url, &mut err);
            if !orig_url.is_null() {
                let mut buf = vec![0u8; 4096];
                if cf_get_fs(orig_url, 1, buf.as_mut_ptr(), buf.len() as CFIndex) != 0 {
                    if let Some(nul_pos) = buf.iter().position(|&b| b == 0) {
                        buf.truncate(nul_pos);
                        use std::os::unix::ffi::OsStringExt;
                        resolved_path = Some(PathBuf::from(std::ffi::OsString::from_vec(buf)));
                    }
                }
                cf_release(orig_url);
            }
        }

        cf_release(url);
        libc::dlclose(sec_lib);
        libc::dlclose(cf_lib);

        resolved_path.unwrap_or_else(|| path.to_path_buf())
    }
}

#[cfg(target_os = "macos")]
fn apply_macos_gui_bundle(zip_bytes: &[u8], current_exe: &Path) -> Result<()> {
    // Find .app bundle path from executable path
    let exe_str = current_exe.to_string_lossy();
    let marker = ".app/";
    let app_path = if let Some(idx) = exe_str.find(marker) {
        PathBuf::from(&exe_str[..idx + marker.len() - 1])
    } else {
        return Err(anyhow!(
            "Not running from an installed macOS .app bundle ({}) - please update manually from the releases page",
            exe_str
        ));
    };

    // If app is currently translocated by Gatekeeper, resolve its real on-disk location
    let app_path = untranslocate_path(&app_path);

    let staging_dir = tempfile_staging_dir("zee-update")?;
    let new_app_path = extract_zip_app_bundle(zip_bytes, &staging_dir)?;
    let pid = std::process::id();

    // Spawn detached helper script to wait for the running app process to exit,
    // swap the .app bundle, remove quarantine attributes, and relaunch the new app.
    let script = r#"APP="$1"
NEW_APP="$2"
STAGING="$3"
PID="$4"

# Wait up to 6 seconds for the running app process to exit cleanly
for i in $(seq 1 30); do
    if ! kill -0 "$PID" 2>/dev/null; then
        break
    fi
    sleep 0.2
done

# If still running, force terminate
if kill -0 "$PID" 2>/dev/null; then
    kill -9 "$PID" 2>/dev/null || true
    sleep 0.2
fi

# Replace the application bundle
rm -rf "$APP"
mv "$NEW_APP" "$APP"

# Strip quarantine and ad-hoc sign so Gatekeeper doesn't block relaunch
xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true
xattr -cr "$APP" 2>/dev/null || true
codesign --force --deep --sign - "$APP" 2>/dev/null || true

# Clean up staging directory
rm -rf "$STAGING"

# Automatically relaunch the updated application!
open -n "$APP"
"#;

    let child = Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .arg("zee-updater")
        .arg(&app_path)
        .arg(&new_app_path)
        .arg(&staging_dir)
        .arg(pid.to_string())
        .spawn()
        .context("Failed to spawn macOS update helper process")?;

    drop(child);
    Ok(())
}

#[allow(dead_code)]
fn extract_zip_app_bundle(zip_bytes: &[u8], dest_dir: &Path) -> Result<PathBuf> {
    use std::io::Cursor;
    let reader = Cursor::new(zip_bytes);
    let mut zip = zip::ZipArchive::new(reader).context("Failed to read ZIP archive")?;

    let mut app_dir_name: Option<String> = None;

    for i in 0..zip.len() {
        let mut file = zip.by_index(i).context("Failed to access file in ZIP")?;
        let raw_name = file.name().to_string();
        
        // Find the root .app folder name
        if let Some(first_component) = raw_name.split('/').next() {
            if first_component.ends_with(".app") {
                app_dir_name = Some(first_component.to_string());
            }
        }

        let outpath = dest_dir.join(&raw_name);

        if file.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                fs::create_dir_all(p)?;
            }
            let mut outfile = File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;
            
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    let _ = fs::set_permissions(&outpath, fs::Permissions::from_mode(mode));
                }
            }
        }
    }

    let app_name = app_dir_name.ok_or_else(|| anyhow!("ZIP archive did not contain a .app bundle"))?;
    Ok(dest_dir.join(app_name))
}

#[cfg(unix)]
fn apply_unix_binary(archive_bytes: &[u8], current_exe: &Path, target_bin_name: &str, is_gui: bool) -> Result<()> {
    let staging_dir = tempfile_staging_dir("zee-update")?;
    let new_bin_path = staging_dir.join(target_bin_name);

    // Extract binary from zip or tar.gz
    if archive_bytes.starts_with(b"PK") {
        extract_zip_binary(archive_bytes, target_bin_name, &new_bin_path)?;
    } else {
        extract_tar_gz_binary(archive_bytes, target_bin_name, &new_bin_path)?;
    }

    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&new_bin_path, fs::Permissions::from_mode(0o755))
        .context("Failed to set executable permissions on updated binary")?;

    let pid = std::process::id();
    let relaunch_cmd = if is_gui { "open -n \"$EXE\" 2>/dev/null || \"$EXE\" &" } else { "" };

    let script = format!(r#"EXE="$1"
NEW_EXE="$2"
STAGING="$3"
PID="$4"

for i in $(seq 1 30); do
    if ! kill -0 "$PID" 2>/dev/null; then
        break
    fi
    sleep 0.2
done

if kill -0 "$PID" 2>/dev/null; then
    kill -9 "$PID" 2>/dev/null || true
    sleep 0.2
fi

rm -f "$EXE"
mv "$NEW_EXE" "$EXE"
chmod +x "$EXE"
xattr -d com.apple.quarantine "$EXE" 2>/dev/null || true
codesign --force --sign - "$EXE" 2>/dev/null || true
rm -rf "$STAGING"

{}
"#, relaunch_cmd);

    let child = Command::new("/bin/sh")
        .arg("-c")
        .arg(&script)
        .arg("zee-updater")
        .arg(current_exe)
        .arg(&new_bin_path)
        .arg(&staging_dir)
        .arg(pid.to_string())
        .spawn()
        .context("Failed to spawn update helper script")?;

    drop(child);
    Ok(())
}

#[allow(dead_code)]
fn extract_tar_gz_binary(tar_gz_bytes: &[u8], target_name: &str, dest_file: &Path) -> Result<()> {
    use flate2::read::GzDecoder;
    use std::io::Cursor;
    use tar::Archive;

    let tar_data = GzDecoder::new(Cursor::new(tar_gz_bytes));
    let mut archive = Archive::new(tar_data);

    for entry in archive.entries().context("Failed to read tar archive entries")? {
        let mut entry = entry.context("Failed to read tar archive entry")?;
        let path = entry.path().context("Invalid entry path in tar archive")?;
        
        if let Some(file_name) = path.file_name() {
            if file_name == target_name {
                let mut out = File::create(dest_file)
                    .context("Failed to create destination file for extracted binary")?;
                io::copy(&mut entry, &mut out)?;
                return Ok(());
            }
        }
    }

    Err(anyhow!("Binary '{}' not found in update archive", target_name))
}

#[cfg(target_os = "windows")]
fn apply_windows_binary(zip_bytes: &[u8], current_exe: &Path, target_bin_name: &str, is_gui: bool) -> Result<()> {
    let staging_dir = tempfile_staging_dir("zee-update")?;
    let new_bin_path = staging_dir.join(target_bin_name);

    extract_zip_binary(zip_bytes, target_bin_name, &new_bin_path)?;

    let pid = std::process::id();
    let relaunch_cmd = if is_gui { "start \"\" \"%EXE%\"" } else { "" };
    let old_exe = current_exe.with_extension("old");

    let script = format!(r#"@echo off
setlocal
set "EXE=%~1"
set "NEWEXE=%~2"
set "OLDEXE=%~3"
set "PID=%~4"

ping -n 2 127.0.0.1 <nul >nul 2>&1
taskkill /F /PID %PID% <nul >nul 2>&1
ping -n 2 127.0.0.1 <nul >nul 2>&1

if exist "%OLDEXE%" del /f /q "%OLDEXE%" >nul 2>&1

for /L %%i in (1,1,10) do (
    if exist "%EXE%" move /y "%EXE%" "%OLDEXE%" >nul 2>&1
    if exist "%NEWEXE%" move /y "%NEWEXE%" "%EXE%" >nul 2>&1
    if not exist "%NEWEXE%" goto swapped
    ping -n 2 127.0.0.1 <nul >nul 2>&1
)

if exist "%OLDEXE%" move /y "%OLDEXE%" "%EXE%" >nul 2>&1
if exist "%EXE%" {}
del /f /q "%~f0" >nul 2>&1
exit /b 1

:swapped
{}
if exist "%OLDEXE%" del /f /q "%OLDEXE%" >nul 2>&1
del /f /q "%~f0" >nul 2>&1
"#, relaunch_cmd, relaunch_cmd);

    let script_path = std::env::temp_dir().join(format!("zee-update-{}.bat", pid));
    fs::write(&script_path, script.replace("\n", "\r\n"))?;

    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    const DETACHED_PROCESS: u32 = 0x00000008;

    let child = Command::new("cmd")
        .args(["/c", script_path.to_str().unwrap(), current_exe.to_str().unwrap(), new_bin_path.to_str().unwrap(), old_exe.to_str().unwrap(), &pid.to_string()])
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .context("Failed to spawn Windows update helper process")?;

    drop(child);
    Ok(())
}

#[allow(dead_code)]
fn extract_zip_binary(zip_bytes: &[u8], target_name: &str, dest_file: &Path) -> Result<()> {
    use std::io::Cursor;
    let reader = Cursor::new(zip_bytes);
    let mut zip = zip::ZipArchive::new(reader).context("Failed to read ZIP archive")?;

    for i in 0..zip.len() {
        let mut file = zip.by_index(i).context("Failed to access file in ZIP")?;
        let raw_name = file.name();
        
        if raw_name.ends_with(target_name) {
            let mut out = File::create(dest_file)
                .context("Failed to create destination file for extracted binary")?;
            io::copy(&mut file, &mut out)?;
            return Ok(());
        }
    }

    Err(anyhow!("Binary '{}' not found in update ZIP archive", target_name))
}

fn tempfile_staging_dir(prefix: &str) -> Result<PathBuf> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), timestamp));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison() {
        // Test updating from older (e.g. 2nd newest 0.0.9 / 0.1.0) to latest
        assert!(is_newer("0.0.9", "0.1.0"));
        assert!(is_newer("0.1.0", "0.1.1"));
        assert!(is_newer("0.1.0", "0.2.0"));
        assert!(is_newer("0.1.0", "1.0.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.1", "0.1.0"));
        assert!(!is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("v0.1.0", "v0.1.1"));
    }

    #[test]
    fn test_all_platforms_asset_resolution() {
        let release_assets = vec![
            ReleaseAsset { name: "zeeg-macos-arm64.zip".to_string(), browser_download_url: "https://example.com/zeeg-macos-arm64.zip".to_string(), size: 1000 },
            ReleaseAsset { name: "zee-macos-arm64.tar.gz".to_string(), browser_download_url: "https://example.com/zee-macos-arm64.tar.gz".to_string(), size: 2000 },
            ReleaseAsset { name: "zeeg-linux-x64.tar.gz".to_string(), browser_download_url: "https://example.com/zeeg-linux-x64.tar.gz".to_string(), size: 3000 },
            ReleaseAsset { name: "zee-linux-x64.tar.gz".to_string(), browser_download_url: "https://example.com/zee-linux-x64.tar.gz".to_string(), size: 4000 },
            ReleaseAsset { name: "zeeg-linux-arm64.tar.gz".to_string(), browser_download_url: "https://example.com/zeeg-linux-arm64.tar.gz".to_string(), size: 5000 },
            ReleaseAsset { name: "zee-linux-arm64.tar.gz".to_string(), browser_download_url: "https://example.com/zee-linux-arm64.tar.gz".to_string(), size: 6000 },
            ReleaseAsset { name: "zeeg-windows-x64.zip".to_string(), browser_download_url: "https://example.com/zeeg-windows-x64.zip".to_string(), size: 7000 },
            ReleaseAsset { name: "zee-windows-x64.zip".to_string(), browser_download_url: "https://example.com/zee-windows-x64.zip".to_string(), size: 8000 },
            ReleaseAsset { name: "zeeg-windows-arm64.zip".to_string(), browser_download_url: "https://example.com/zeeg-windows-arm64.zip".to_string(), size: 9000 },
            ReleaseAsset { name: "zee-windows-arm64.zip".to_string(), browser_download_url: "https://example.com/zee-windows-arm64.zip".to_string(), size: 10000 },
        ];

        // macOS arm64
        let mac_gui = find_matching_asset_for_platform(&release_assets, AppType::Gui, "macos", "aarch64").unwrap();
        assert_eq!(mac_gui.name, "zeeg-macos-arm64.zip");
        let mac_cli = find_matching_asset_for_platform(&release_assets, AppType::Cli, "macos", "aarch64").unwrap();
        assert_eq!(mac_cli.name, "zee-macos-arm64.tar.gz");

        // Linux x64
        let lin_gui_x64 = find_matching_asset_for_platform(&release_assets, AppType::Gui, "linux", "x86_64").unwrap();
        assert_eq!(lin_gui_x64.name, "zeeg-linux-x64.tar.gz");
        let lin_cli_x64 = find_matching_asset_for_platform(&release_assets, AppType::Cli, "linux", "x86_64").unwrap();
        assert_eq!(lin_cli_x64.name, "zee-linux-x64.tar.gz");

        // Linux arm64
        let lin_gui_arm64 = find_matching_asset_for_platform(&release_assets, AppType::Gui, "linux", "aarch64").unwrap();
        assert_eq!(lin_gui_arm64.name, "zeeg-linux-arm64.tar.gz");
        let lin_cli_arm64 = find_matching_asset_for_platform(&release_assets, AppType::Cli, "linux", "aarch64").unwrap();
        assert_eq!(lin_cli_arm64.name, "zee-linux-arm64.tar.gz");

        // Windows x64
        let win_gui_x64 = find_matching_asset_for_platform(&release_assets, AppType::Gui, "windows", "x86_64").unwrap();
        assert_eq!(win_gui_x64.name, "zeeg-windows-x64.zip");
        let win_cli_x64 = find_matching_asset_for_platform(&release_assets, AppType::Cli, "windows", "x86_64").unwrap();
        assert_eq!(win_cli_x64.name, "zee-windows-x64.zip");

        // Windows arm64
        let win_gui_arm64 = find_matching_asset_for_platform(&release_assets, AppType::Gui, "windows", "aarch64").unwrap();
        assert_eq!(win_gui_arm64.name, "zeeg-windows-arm64.zip");
        let win_cli_arm64 = find_matching_asset_for_platform(&release_assets, AppType::Cli, "windows", "aarch64").unwrap();
        assert_eq!(win_cli_arm64.name, "zee-windows-arm64.zip");

        // Verify new zee-GUI- naming format
        let modern_assets = vec![
            ReleaseAsset { name: "zee-GUI-v0.1.18-macos-arm64.zip".to_string(), browser_download_url: "https://example.com/mac".to_string(), size: 1000 },
            ReleaseAsset { name: "zee-v0.1.18-linux-x64.zip".to_string(), browser_download_url: "https://example.com/lin".to_string(), size: 2000 },
            ReleaseAsset { name: "zee-GUI-v0.1.18-windows-x64.zip".to_string(), browser_download_url: "https://example.com/win".to_string(), size: 3000 },
        ];
        let mac_gui_modern = find_matching_asset_for_platform(&modern_assets, AppType::Gui, "macos", "aarch64").unwrap();
        assert_eq!(mac_gui_modern.name, "zee-GUI-v0.1.18-macos-arm64.zip");
        let lin_cli_modern = find_matching_asset_for_platform(&modern_assets, AppType::Cli, "linux", "x86_64").unwrap();
        assert_eq!(lin_cli_modern.name, "zee-v0.1.18-linux-x64.zip");
        let win_gui_modern = find_matching_asset_for_platform(&modern_assets, AppType::Gui, "windows", "x86_64").unwrap();
        assert_eq!(win_gui_modern.name, "zee-GUI-v0.1.18-windows-x64.zip");
    }

    #[test]
    fn test_tar_gz_extraction() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use tar::Builder;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        {
            let mut tar = Builder::new(&mut encoder);
            let test_content = b"fake binary executable content for zee v0.1.1";
            let mut header = tar::Header::new_gnu();
            header.set_path("zee").unwrap();
            header.set_size(test_content.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append(&header, &test_content[..]).unwrap();
            tar.finish().unwrap();
        }
        let tar_gz_bytes = encoder.finish().unwrap();

        let temp_dir = tempfile_staging_dir("test-tar-extract").unwrap();
        let dest_file = temp_dir.join("zee");

        extract_tar_gz_binary(&tar_gz_bytes, "zee", &dest_file).unwrap();
        assert!(dest_file.exists());
        let read_back = fs::read(&dest_file).unwrap();
        assert_eq!(read_back, b"fake binary executable content for zee v0.1.1");
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_zip_app_bundle_extraction() {
        use zip::write::{SimpleFileOptions, ZipWriter};
        use std::io::{Cursor, Write};

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();
            
            zip.add_directory("zee.app", options).unwrap();
            zip.add_directory("zee.app/Contents", options).unwrap();
            zip.add_directory("zee.app/Contents/MacOS", options).unwrap();
            zip.start_file("zee.app/Contents/MacOS/zeeg", options).unwrap();
            zip.write_all(b"zeeg macos gui binary").unwrap();
            zip.finish().unwrap();
        }

        let temp_dir = tempfile_staging_dir("test-zip-app-extract").unwrap();
        let app_dir = extract_zip_app_bundle(&buf, &temp_dir).unwrap();
        assert!(app_dir.exists());
        assert_eq!(app_dir.file_name().unwrap(), "zee.app");
        assert!(app_dir.join("Contents/MacOS/zeeg").exists());
        let content = fs::read(app_dir.join("Contents/MacOS/zeeg")).unwrap();
        assert_eq!(content, b"zeeg macos gui binary");
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_zip_windows_binary_extraction() {
        use zip::write::{SimpleFileOptions, ZipWriter};
        use std::io::{Cursor, Write};

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();
            zip.start_file("zee.exe", options).unwrap();
            zip.write_all(b"windows binary data").unwrap();
            zip.finish().unwrap();
        }

        let temp_dir = tempfile_staging_dir("test-zip-win-extract").unwrap();
        let dest = temp_dir.join("zee.exe");
        extract_zip_binary(&buf, "zee.exe", &dest).unwrap();
        assert!(dest.exists());
        assert_eq!(fs::read(&dest).unwrap(), b"windows binary data");
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_untranslocate_path() {
        // Non-translocated path returns itself unchanged
        let normal_path = PathBuf::from("/Applications/Zee.app");
        assert_eq!(untranslocate_path(&normal_path), normal_path);
    }
}
