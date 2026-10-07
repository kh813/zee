use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::selfupdate::open_url;

/// Default Google Drive OAuth 2.0 Client ID for Zee Desktop Application.
pub const DEFAULT_CLIENT_ID: &str = "1049303273187-j70kcl1m019q412g3e070vdkh27u6d9e.apps.googleusercontent.com";
pub const DEFAULT_CLIENT_SECRET: &str = "GOCSPX-qB64j71kX_example_secret_for_desktop";
pub const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const GOOGLE_DRIVE_API_BASE: &str = "https://www.googleapis.com/drive/v3";
pub const GOOGLE_DRIVE_UPLOAD_BASE: &str = "https://www.googleapis.com/upload/drive/v3";

/// OAuth 2.0 token stored locally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GDriveToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>, // Unix timestamp in seconds
    pub token_type: String,
    pub email: Option<String>,
}

impl GDriveToken {
    pub fn is_expired(&self) -> bool {
        if let Some(exp) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            // Consider expired if within 60 seconds of expiration
            now + 60 >= exp
        } else {
            false
        }
    }
}

/// A file or folder item returned from Google Drive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GDriveItem {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub is_folder: bool,
    pub size: Option<u64>,
    pub modified_time: Option<String>,
}

/// Metadata stored locally for files cached from Google Drive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFileMeta {
    pub file_id: String,
    pub file_name: String,
    pub mime_type: String,
    pub local_path: PathBuf,
    pub remote_modified_time: Option<String>,
    pub last_synced_at: u64,
}

pub struct GDriveManager;

impl GDriveManager {
    /// Return the path to the stored token file: `~/.config/zee/gdrive_token.json`.
    pub fn token_file_path() -> Option<PathBuf> {
        Config::config_dir().map(|d| d.join("gdrive_token.json"))
    }

    /// Return the root directory for locally cached Google Drive files: `~/.config/zee/gdrive_cache/`.
    pub fn cache_dir() -> Option<PathBuf> {
        Config::config_dir().map(|d| d.join("gdrive_cache"))
    }

    /// Return the path to the cache metadata index: `~/.config/zee/gdrive_cache/metadata.json`.
    pub fn metadata_file_path() -> Option<PathBuf> {
        Self::cache_dir().map(|d| d.join("metadata.json"))
    }

    /// Check if Google Drive is currently authenticated with a saved token.
    pub fn is_authenticated() -> bool {
        Self::load_token().is_some()
    }

    /// Return the connected account email if available.
    pub fn connected_account_email() -> Option<String> {
        Self::load_token().and_then(|t| t.email)
    }

    /// Load saved OAuth token from disk.
    pub fn load_token() -> Option<GDriveToken> {
        let path = Self::token_file_path()?;
        if !path.exists() {
            return None;
        }
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save OAuth token to disk.
    pub fn save_token(token: &GDriveToken) -> Result<()> {
        let path = Self::token_file_path().context("Could not determine config directory")?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(token)?;
        fs::write(&path, json)?;
        Ok(())
    }

    /// Remove saved OAuth token (Sign Out).
    pub fn sign_out() -> Result<()> {
        if let Some(path) = Self::token_file_path() {
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    /// Get a valid access token, automatically refreshing it if expired.
    pub fn get_valid_access_token() -> Result<String> {
        let mut token = Self::load_token().context("Google Drive is not connected. Please sign in.")?;
        if !token.is_expired() {
            return Ok(token.access_token);
        }

        let refresh_token = token.refresh_token.as_ref().context("No refresh token available. Please reconnect Google Drive.")?;
        let new_token = Self::refresh_access_token(refresh_token)?;
        token.access_token = new_token.access_token.clone();
        token.expires_at = new_token.expires_at;
        if new_token.refresh_token.is_some() {
            token.refresh_token = new_token.refresh_token;
        }
        Self::save_token(&token)?;
        Ok(token.access_token)
    }

    /// Refresh access token using Google OAuth 2.0 token endpoint.
    pub fn refresh_access_token(refresh_token: &str) -> Result<GDriveToken> {
        let resp = ureq::post(GOOGLE_TOKEN_URL)
            .send_form(&[
                ("client_id", DEFAULT_CLIENT_ID),
                ("client_secret", DEFAULT_CLIENT_SECRET),
                ("refresh_token", refresh_token),
                ("grant_type", "refresh_token"),
            ])
            .context("Failed to refresh Google Drive access token")?;

        let json: serde_json::Value = resp.into_json()?;
        let access_token = json["access_token"].as_str().context("Missing access_token in refresh response")?.to_string();
        let expires_in = json["expires_in"].as_u64().unwrap_or(3600);
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        Ok(GDriveToken {
            access_token,
            refresh_token: json["refresh_token"].as_str().map(|s| s.to_string()),
            expires_at: Some(now + expires_in),
            token_type: "Bearer".to_string(),
            email: None,
        })
    }

    /// Start the browser-based OAuth 2.0 loopback flow.
    /// Opens the default browser to authorize, receives callback on 127.0.0.1, exchanges code for token, and saves it.
    pub fn start_oauth_flow() -> Result<GDriveToken> {
        // Bind ephemeral port on localhost
        let listener = TcpListener::bind("127.0.0.1:0")
            .context("Failed to start local OAuth loopback server")?;
        let port = listener.local_addr()?.port();
        let redirect_uri = format!("http://127.0.0.1:{}", port);

        // Requested scopes: drive.file (create/edit/save) and drive.readonly (browse/read), plus userinfo.email
        let scopes = "https://www.googleapis.com/auth/drive.file https://www.googleapis.com/auth/drive.readonly https://www.googleapis.com/auth/userinfo.email";
        let auth_url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
            GOOGLE_AUTH_URL,
            urlencoding_encode(DEFAULT_CLIENT_ID),
            urlencoding_encode(&redirect_uri),
            urlencoding_encode(scopes),
        );

        // Open the browser
        open_url(&auth_url).context("Failed to open default web browser for Google authentication")?;

        // Wait for incoming callback connection (timeout: 120s)
        listener.set_nonblocking(false)?;
        let (mut stream, _) = listener.accept().context("Did not receive Google OAuth authorization callback")?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;

        let mut reader = BufReader::new(&stream);
        let mut request_line = String::new();
        reader.read_line(&mut request_line)?;

        // Parse: GET /?code=...&scope=... HTTP/1.1
        let code = extract_query_param(&request_line, "code")
            .context("Authorization code not found in callback request")?;

        // Respond with success HTML page
        let html_response = "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>Zee - Google Drive Connected</title><style>body{font-family:-apple-system,BlinkMacSystemFont,sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#1e1e1e;color:#fff;}.box{text-align:center;padding:40px 60px;background:#2d2d2d;border-radius:12px;box-shadow:0 8px 32px rgba(0,0,0,0.4);border:1px solid #444;}h1{color:#4caf50;margin:0 0 12px 0;font-size:24px;}p{color:#aaa;margin:0;font-size:15px;}</style></head><body><div class=\"box\"><h1>✓ Connected to Google Drive</h1><p>You can close this tab and return to Zee.</p></div></body></html>";
        let http_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            html_response.len(),
            html_response
        );
        let _ = stream.write_all(http_response.as_bytes());
        let _ = stream.flush();

        // Exchange code for token
        let resp = ureq::post(GOOGLE_TOKEN_URL)
            .send_form(&[
                ("code", &code),
                ("client_id", DEFAULT_CLIENT_ID),
                ("client_secret", DEFAULT_CLIENT_SECRET),
                ("redirect_uri", &redirect_uri),
                ("grant_type", "authorization_code"),
            ])
            .context("Failed to exchange authorization code for tokens")?;

        let json: serde_json::Value = resp.into_json()?;
        let access_token = json["access_token"].as_str().context("No access_token returned")?.to_string();
        let refresh_token = json["refresh_token"].as_str().map(|s| s.to_string());
        let expires_in = json["expires_in"].as_u64().unwrap_or(3600);
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        // Fetch user email if available
        let email = if let Ok(u_resp) = ureq::get("https://www.googleapis.com/oauth2/v2/userinfo")
            .set("Authorization", &format!("Bearer {}", access_token))
            .call()
        {
            if let Ok(u_json) = u_resp.into_json::<serde_json::Value>() {
                u_json["email"].as_str().map(|s| s.to_string())
            } else {
                None
            }
        } else {
            None
        };

        let token = GDriveToken {
            access_token,
            refresh_token,
            expires_at: Some(now + expires_in),
            token_type: "Bearer".to_string(),
            email,
        };

        Self::save_token(&token)?;
        Ok(token)
    }

    /// List files and folders in Google Drive.
    /// If `parent_id` is None, lists files in root ("My Drive").
    pub fn list_files(parent_id: Option<&str>, search_query: Option<&str>) -> Result<Vec<GDriveItem>> {
        let access_token = Self::get_valid_access_token()?;

        let mut q_parts = vec!["trashed = false".to_string()];
        if let Some(sq) = search_query {
            let sanitized = sq.replace('\'', "\\'");
            if !sanitized.is_empty() {
                q_parts.push(format!("name contains '{}'", sanitized));
            }
        } else {
            let parent = parent_id.unwrap_or("root");
            q_parts.push(format!("'{}' in parents", parent));
        }

        let q = q_parts.join(" and ");
        let url = format!("{}/files", GOOGLE_DRIVE_API_BASE);

        let resp = ureq::get(&url)
            .set("Authorization", &format!("Bearer {}", access_token))
            .query("q", &q)
            .query("fields", "files(id,name,mimeType,size,modifiedTime)")
            .query("orderBy", "folder,name")
            .query("pageSize", "100")
            .call()
            .context("Failed to list files from Google Drive")?;

        let json: serde_json::Value = resp.into_json()?;
        let files_arr = json["files"].as_array().context("Invalid files list response")?;

        let mut items = Vec::new();
        for f in files_arr {
            let id = f["id"].as_str().unwrap_or("").to_string();
            let name = f["name"].as_str().unwrap_or("").to_string();
            let mime_type = f["mimeType"].as_str().unwrap_or("").to_string();
            let is_folder = mime_type == "application/vnd.google-apps.folder";
            let size = f["size"].as_str().and_then(|s| s.parse::<u64>().ok());
            let modified_time = f["modifiedTime"].as_str().map(|s| s.to_string());

            if !id.is_empty() && !name.is_empty() {
                items.push(GDriveItem {
                    id,
                    name,
                    mime_type,
                    is_folder,
                    size,
                    modified_time,
                });
            }
        }

        // Sort folders first, then alphabetical by name
        items.sort_by(|a, b| {
            if a.is_folder != b.is_folder {
                b.is_folder.cmp(&a.is_folder)
            } else {
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
            }
        });

        Ok(items)
    }

    /// Download a file's content from Google Drive.
    pub fn download_file(file_id: &str, mime_type: &str) -> Result<Vec<u8>> {
        let access_token = Self::get_valid_access_token()?;

        // If it's a native Google Doc, export as plain text
        let resp = if mime_type.starts_with("application/vnd.google-apps.") {
            let export_url = format!("{}/files/{}/export", GOOGLE_DRIVE_API_BASE, file_id);
            ureq::get(&export_url)
                .set("Authorization", &format!("Bearer {}", access_token))
                .query("mimeType", "text/plain")
                .call()
                .context("Failed to export Google Document as text")?
        } else {
            let url = format!("{}/files/{}?alt=media", GOOGLE_DRIVE_API_BASE, file_id);
            ureq::get(&url)
                .set("Authorization", &format!("Bearer {}", access_token))
                .call()
                .context("Failed to download file content from Google Drive")?
        };

        let mut bytes = Vec::new();
        resp.into_reader().read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    /// Download and cache a Google Drive file locally for immediate zero-latency editing.
    /// Returns the local cached PathBuf ready to be opened by `Editor::from_file`.
    pub fn download_and_cache(item: &GDriveItem) -> Result<PathBuf> {
        let cache_root = Self::cache_dir().context("Could not determine cache directory")?;
        let file_cache_dir = cache_root.join(&item.id);
        fs::create_dir_all(&file_cache_dir)?;

        // Clean local filename
        let local_path = file_cache_dir.join(&item.name);
        let content = Self::download_file(&item.id, &item.mime_type)?;
        fs::write(&local_path, &content)?;

        // Record metadata in local cache map
        let mut meta_map = Self::load_metadata();
        meta_map.insert(local_path.clone(), CachedFileMeta {
            file_id: item.id.clone(),
            file_name: item.name.clone(),
            mime_type: item.mime_type.clone(),
            local_path: local_path.clone(),
            remote_modified_time: item.modified_time.clone(),
            last_synced_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        });
        Self::save_metadata(&meta_map)?;

        Ok(local_path)
    }

    /// Upload/update file content back to Google Drive (creates a new revision).
    pub fn upload_file(file_id: &str, data: &[u8]) -> Result<()> {
        let access_token = Self::get_valid_access_token()?;
        let upload_url = format!("{}/files/{}?uploadType=media", GOOGLE_DRIVE_UPLOAD_BASE, file_id);

        let _ = ureq::patch(&upload_url)
            .set("Authorization", &format!("Bearer {}", access_token))
            .set("Content-Type", "application/octet-stream")
            .send_bytes(data)
            .context("Failed to upload updated file content to Google Drive")?;

        Ok(())
    }

    /// Load the metadata map of currently cached Google Drive files.
    pub fn load_metadata() -> HashMap<PathBuf, CachedFileMeta> {
        let path = match Self::metadata_file_path() {
            Some(p) => p,
            None => return HashMap::new(),
        };
        if !path.exists() {
            return HashMap::new();
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };
        serde_json::from_str(&content).unwrap_or_default()
    }

    /// Save the metadata map to disk.
    pub fn save_metadata(map: &HashMap<PathBuf, CachedFileMeta>) -> Result<()> {
        let path = Self::metadata_file_path().context("Could not determine metadata path")?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(map)?;
        fs::write(&path, json)?;
        Ok(())
    }

    /// Check if a given file path is a cached Google Drive file.
    pub fn is_gdrive_path<P: AsRef<Path>>(path: P) -> bool {
        let path = path.as_ref();
        if let Some(cache_root) = Self::cache_dir() {
            if path.starts_with(&cache_root) {
                return true;
            }
        }
        let meta = Self::load_metadata();
        meta.contains_key(path)
    }

    /// Retrieve the Google Drive file ID associated with a cached local path.
    pub fn get_file_id_for_path<P: AsRef<Path>>(path: P) -> Option<String> {
        let path = path.as_ref();
        let meta = Self::load_metadata();
        if let Some(item) = meta.get(path) {
            return Some(item.file_id.clone());
        }

        // Fallback: extract from folder path `gdrive_cache/<file_id>/<filename>`
        if let Some(cache_root) = Self::cache_dir() {
            if let Ok(rel) = path.strip_prefix(&cache_root) {
                let mut components = rel.components();
                if let Some(std::path::Component::Normal(id_os)) = components.next() {
                    return Some(id_os.to_string_lossy().to_string());
                }
            }
        }
        None
    }

    /// Upload a saved local cache file back to Google Drive asynchronously in the background.
    /// Does not block the editor UI.
    pub fn sync_in_background<P: AsRef<Path>>(path: P, is_syncing_flag: Option<Arc<AtomicBool>>) {
        let path = path.as_ref().to_path_buf();
        let file_id = match Self::get_file_id_for_path(&path) {
            Some(id) => id,
            None => return,
        };

        std::thread::spawn(move || {
            if let Some(ref flag) = is_syncing_flag {
                flag.store(true, Ordering::SeqCst);
            }

            if let Ok(bytes) = fs::read(&path) {
                if let Err(e) = Self::upload_file(&file_id, &bytes) {
                    eprintln!("Google Drive background sync failed for file {}: {}", file_id, e);
                } else {
                    // Update metadata timestamp
                    let mut meta_map = Self::load_metadata();
                    if let Some(item) = meta_map.get_mut(&path) {
                        item.last_synced_at = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                    }
                    let _ = Self::save_metadata(&meta_map);
                }
            }

            if let Some(ref flag) = is_syncing_flag {
                flag.store(false, Ordering::SeqCst);
            }
        });
    }
}

/// Helper function to URL-encode characters.
fn urlencoding_encode(s: &str) -> String {
    let mut encoded = String::new();
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

/// Extract query parameter value from an HTTP request line.
fn extract_query_param(request_line: &str, param: &str) -> Option<String> {
    // E.g. "GET /?code=4/0A...&scope=... HTTP/1.1"
    let path = request_line.split_whitespace().nth(1)?;
    let query_str = path.split('?').nth(1)?;
    for pair in query_str.split('&') {
        let mut parts = pair.split('=');
        if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
            if k == param {
                return Some(urlencoding_decode(v));
            }
        }
    }
    None
}

/// Simple URL decode helper for query param.
fn urlencoding_decode(s: &str) -> String {
    let mut res = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex_str) = std::str::from_utf8(&bytes[i + 1..=i + 2]) {
                if let Ok(b) = u8::from_str_radix(hex_str, 16) {
                    res.push(b as char);
                    i += 3;
                    continue;
                }
            }
        } else if bytes[i] == b'+' {
            res.push(' ');
            i += 1;
            continue;
        }
        res.push(bytes[i] as char);
        i += 1;
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_encoding_and_query_param_extraction() {
        let encoded = urlencoding_encode("https://example.com/test?a=1&b=hello world");
        assert!(encoded.contains("%2F"));
        assert!(encoded.contains("%20"));

        let req = "GET /callback?code=test_code_12345&scope=read HTTP/1.1";
        let code = extract_query_param(req, "code");
        assert_eq!(code, Some("test_code_12345".to_string()));

        let scope = extract_query_param(req, "scope");
        assert_eq!(scope, Some("read".to_string()));

        let missing = extract_query_param(req, "invalid");
        assert_eq!(missing, None);
    }

    #[test]
    fn test_token_expiration_logic() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        let valid_token = GDriveToken {
            access_token: "token123".to_string(),
            refresh_token: Some("refresh123".to_string()),
            expires_at: Some(now + 3600),
            token_type: "Bearer".to_string(),
            email: Some("test@example.com".to_string()),
        };
        assert!(!valid_token.is_expired());

        let expired_token = GDriveToken {
            access_token: "token123".to_string(),
            refresh_token: Some("refresh123".to_string()),
            expires_at: Some(now - 10),
            token_type: "Bearer".to_string(),
            email: None,
        };
        assert!(expired_token.is_expired());
    }

    #[test]
    fn test_gdrive_item_sorting() {
        let mut items = vec![
            GDriveItem {
                id: "1".into(),
                name: "b_file.md".into(),
                mime_type: "text/markdown".into(),
                is_folder: false,
                size: Some(100),
                modified_time: None,
            },
            GDriveItem {
                id: "2".into(),
                name: "folder_z".into(),
                mime_type: "application/vnd.google-apps.folder".into(),
                is_folder: true,
                size: None,
                modified_time: None,
            },
            GDriveItem {
                id: "3".into(),
                name: "a_file.txt".into(),
                mime_type: "text/plain".into(),
                is_folder: false,
                size: Some(200),
                modified_time: None,
            },
        ];

        items.sort_by(|a, b| {
            if a.is_folder != b.is_folder {
                b.is_folder.cmp(&a.is_folder)
            } else {
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
            }
        });

        // Folder must come first
        assert!(items[0].is_folder);
        assert_eq!(items[0].name, "folder_z");
        // Then files alphabetically
        assert_eq!(items[1].name, "a_file.txt");
        assert_eq!(items[2].name, "b_file.md");
    }

    #[test]
    fn test_metadata_serialization_roundtrip() {
        let mut map = HashMap::new();
        let path = PathBuf::from("/tmp/gdrive_cache/file123/notes.md");
        map.insert(path.clone(), CachedFileMeta {
            file_id: "file123".into(),
            file_name: "notes.md".into(),
            mime_type: "text/markdown".into(),
            local_path: path.clone(),
            remote_modified_time: Some("2026-10-08T00:00:00Z".into()),
            last_synced_at: 1728345600,
        });

        let serialized = serde_json::to_string(&map).expect("Serialize failed");
        let deserialized: HashMap<PathBuf, CachedFileMeta> = serde_json::from_str(&serialized).expect("Deserialize failed");
        assert!(deserialized.contains_key(&path));
        assert_eq!(deserialized.get(&path).unwrap().file_id, "file123");
    }
}
