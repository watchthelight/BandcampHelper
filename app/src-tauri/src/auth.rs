use std::sync::Arc;
use reqwest::cookie::Jar;
use tauri::{Emitter, Manager};

use crate::BandcampState;
use crate::models::{AuthStatus, CollectionItem, CollectionResult};

/// Try to restore session from persisted cookies (no login window needed).
#[tauri::command]
pub async fn try_restore_session(
    state: tauri::State<'_, BandcampState>,
    app: tauri::AppHandle,
) -> Result<usize, String> {
    let login_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?
        .join("LoginSession");

    let jar = Arc::new(Jar::default());
    let count = extract_webview2_cookies(&login_data_dir, &jar).unwrap_or(0);

    if count == 0 {
        return Err("no saved session".to_string());
    }

    let client = reqwest::Client::builder()
        .cookie_provider(jar.clone())
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .map_err(|e| format!("http client: {e}"))?;

    *state.cookies.lock().await = Some(jar);
    *state.client.lock().await = Some(client);

    Ok(count)
}

/// Open WebView to Bandcamp login, wait for auth, extract cookies from WebView2 DB.
#[tauri::command]
pub async fn login(
    state: tauri::State<'_, BandcampState>,
    app: tauri::AppHandle,
) -> Result<usize, String> {
    use tauri::WebviewWindowBuilder;

    // Close existing login window
    if let Some(w) = app.get_webview_window("bc-login") {
        let _ = w.destroy();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    // Separate data dir so its WebView2 process can release the cookie DB
    let login_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?
        .join("LoginSession");

    let _ = app.emit("login-status", "opening bandcamp login...");

    let _win = WebviewWindowBuilder::new(
        &app,
        "bc-login",
        tauri::WebviewUrl::External("https://bandcamp.com/login".parse().unwrap()),
    )
    .title("bandcamp — log in")
    .inner_size(520.0, 640.0)
    .center()
    .data_directory(login_data_dir.clone())
    .build()
    .map_err(|e| format!("create login window: {e}"))?;

    let _ = app.emit("login-status", "log in to bandcamp in the window...");
    eprintln!("[login] window opened, waiting for login...");

    // Poll URL until user navigates away from /login
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let w = match app.get_webview_window("bc-login") {
            Some(w) => w,
            None => return Err("login window was closed".to_string()),
        };
        let url = w.url().map(|u| u.to_string()).unwrap_or_default();
        if !url.contains("/login") && url.contains("bandcamp.com") {
            eprintln!("[login] detected redirect: {}", url);
            let _ = app.emit("login-status", "logged in, closing window...");
            // Wait for cookies to fully persist to disk
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            break;
        }
    }

    // Close login window — its WebView2 process should exit (separate data dir)
    if let Some(w) = app.get_webview_window("bc-login") {
        let _ = w.destroy();
    }
    let _ = app.emit("login-status", "extracting cookies...");
    eprintln!("[login] window closed, waiting for DB release...");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Extract cookies from the login session's WebView2 DB
    let jar = Arc::new(Jar::default());
    let count = extract_webview2_cookies(&login_data_dir, &jar)?;
    eprintln!("[login] extracted {} bandcamp cookies", count);

    if count == 0 {
        return Err("no bandcamp cookies found — login may have failed".to_string());
    }

    // Build authenticated client
    let client = reqwest::Client::builder()
        .cookie_provider(jar.clone())
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .map_err(|e| format!("http client: {e}"))?;

    *state.cookies.lock().await = Some(jar);
    *state.client.lock().await = Some(client);

    let _ = app.emit("login-status", format!("authenticated ({} cookies)", count));
    Ok(count)
}

fn extract_webview2_cookies(
    login_data_dir: &std::path::Path,
    jar: &Arc<Jar>,
) -> Result<usize, String> {
    use rusqlite::Connection;

    // Find cookie DB
    let cookies_path = login_data_dir
        .join("EBWebView").join("Default").join("Network").join("Cookies");

    if !cookies_path.exists() {
        return Err(format!("cookie DB not found: {}", cookies_path.display()));
    }

    eprintln!("[cookies] reading: {}", cookies_path.display());

    // Read master key from Local State
    let local_state_path = login_data_dir.join("EBWebView").join("Local State");
    let master_key = read_master_key(&local_state_path)?;
    eprintln!("[cookies] master key: {} bytes", master_key.len());

    // Copy DB to temp (retry up to 5 times for lock release)
    let tmp = std::env::temp_dir().join("bc_login_cookies.sqlite");
    let mut copied = false;
    for attempt in 0..8 {
        match std::fs::copy(&cookies_path, &tmp) {
            Ok(_) => { copied = true; break; }
            Err(e) => {
                eprintln!("[cookies] copy attempt {}: {}", attempt, e);
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }
    if !copied {
        return Err("cookie DB still locked after 8 retries".to_string());
    }
    eprintln!("[cookies] DB copied");

    let conn = Connection::open_with_flags(
        &tmp,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("open DB: {e}"))?;

    let mut count = 0usize;
    {
        let mut stmt = conn
            .prepare("SELECT host_key, name, value, encrypted_value FROM cookies WHERE host_key LIKE '%bandcamp.com%'")
            .map_err(|e| format!("query: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            })
            .map_err(|e| format!("query: {e}"))?;

        for row in rows {
            if let Ok((host, name, value, enc_value)) = row {
                let real_value = if !value.is_empty() {
                    value
                } else if enc_value.len() > 3 {
                    match decrypt_cookie(&master_key, &enc_value) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("[cookies] decrypt failed: {} — {}", name, e);
                            continue;
                        }
                    }
                } else {
                    continue;
                };

                if real_value.is_empty() { continue; }

                let url_str = format!("https://{}", host.trim_start_matches('.'));
                if let Ok(url) = url_str.parse::<url::Url>() {
                    // Log first bytes as hex to verify decryption
                    let hex: String = real_value.bytes().take(16).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
                    eprintln!("[cookies] {} = [{}] {}...", name, hex, &real_value[..real_value.len().min(30)]);

                    // Include Domain=.bandcamp.com so cookies are sent to subdomains
                    // (popplers5.bandcamp.com etc for downloads)
                    jar.add_cookie_str(
                        &format!("{}={}; Domain=.bandcamp.com; Path=/", name, real_value),
                        &url,
                    );
                    count += 1;
                }
            }
        }
    }

    drop(conn);
    let _ = std::fs::remove_file(&tmp);

    Ok(count)
}

// ── Crypto helpers ──────────────────────────────────────────────────────

fn read_master_key(local_state_path: &std::path::Path) -> Result<Vec<u8>, String> {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    let text = std::fs::read_to_string(local_state_path)
        .map_err(|e| format!("read Local State: {e}"))?;
    let data: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("parse Local State: {e}"))?;

    let enc_key_b64 = data["os_crypt"]["encrypted_key"]
        .as_str()
        .ok_or("no encrypted_key")?;
    let enc_key_raw = STANDARD.decode(enc_key_b64).map_err(|e| format!("base64: {e}"))?;

    let enc_key = if enc_key_raw.starts_with(b"DPAPI") { &enc_key_raw[5..] } else { &enc_key_raw };
    dpapi_decrypt(enc_key)
}

fn decrypt_cookie(master_key: &[u8], encrypted: &[u8]) -> Result<String, String> {
    use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
    use aes_gcm::Nonce;

    if encrypted.len() < 3 + 12 + 16 { return Err("too short".to_string()); }

    let prefix = &encrypted[..3];
    if prefix == b"v10" || prefix == b"v11" || prefix == b"v12" {
        let nonce = Nonce::from_slice(&encrypted[3..15]);
        let ciphertext = &encrypted[15..];
        let cipher = Aes256Gcm::new_from_slice(master_key).map_err(|e| format!("aes: {e}"))?;
        let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|e| format!("decrypt: {e}"))?;
        // Chrome/WebView2 prefixes decrypted values with a 32-byte domain hash.
        // Strip it to get the actual cookie value.
        let value_bytes = if plaintext.len() > 32 {
            &plaintext[32..]
        } else {
            &plaintext
        };
        return Ok(String::from_utf8_lossy(value_bytes).to_string());
    }

    let decrypted = dpapi_decrypt(encrypted)?;
    Ok(String::from_utf8_lossy(&decrypted).to_string())
}

#[cfg(target_os = "windows")]
fn dpapi_decrypt(data: &[u8]) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    use windows_sys::Win32::Foundation::LocalFree;

    unsafe {
        let mut input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };

        if CryptUnprotectData(&mut input, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0, &mut output) == 0 {
            return Err("DPAPI failed".to_string());
        }

        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as _);
        Ok(result)
    }
}

#[cfg(not(target_os = "windows"))]
fn dpapi_decrypt(_data: &[u8]) -> Result<Vec<u8>, String> {
    Err("DPAPI not available".to_string())
}

// ── Bandcamp API commands ───────────────────────────────────────────────

#[tauri::command]
pub async fn check_auth(
    username: String,
    state: tauri::State<'_, BandcampState>,
) -> Result<AuthStatus, String> {
    let client = state.client.lock().await;
    let client = client.as_ref().ok_or("not logged in")?;

    let url = format!("https://bandcamp.com/{}", username);
    let resp = client.get(&url).send().await.map_err(|e| format!("fetch: {e}"))?;
    let text = resp.text().await.map_err(|e| format!("read: {e}"))?;

    let data = parse_pagedata(&text);
    if data.is_null() || data.get("fan_data").is_none() {
        return Ok(AuthStatus { authenticated: false, username, collection_count: 0 });
    }

    let collection_count = data["collection_count"].as_u64().unwrap_or(0) as usize;
    Ok(AuthStatus { authenticated: true, username, collection_count })
}

#[tauri::command]
pub async fn fetch_collection(
    username: String,
    state: tauri::State<'_, BandcampState>,
    app: tauri::AppHandle,
) -> Result<CollectionResult, String> {
    let client = {
        let guard = state.client.lock().await;
        guard.as_ref().ok_or("not logged in")?.clone()
    };

    let url = format!("https://bandcamp.com/{}", username);
    let resp = client.get(&url).send().await.map_err(|e| format!("fetch: {e}"))?;
    let text = resp.text().await.map_err(|e| format!("read: {e}"))?;

    let data = parse_pagedata(&text);
    if data.is_null() { return Err("could not parse page".to_string()); }

    let fan_id = data["fan_data"]["fan_id"].as_u64().ok_or("no fan_id — not authenticated")?;
    let collection_count = data["collection_count"].as_u64().unwrap_or(0) as usize;

    let mut items = Vec::new();
    let redownload_urls = &data["collection_data"]["redownload_urls"];

    if let Some(cache) = data["item_cache"]["collection"].as_object() {
        for (_key, item) in cache {
            if let Some(ci) = parse_collection_item(item, redownload_urls) {
                items.push(ci);
            }
        }
    }

    let total_items = data["collection_data"]["item_count"].as_u64().unwrap_or(0) as usize;
    let mut last_token = data["collection_data"]["last_token"].as_str().unwrap_or("").to_string();
    let mut remaining = total_items.saturating_sub(items.len());

    while remaining > 0 {
        let payload = serde_json::json!({
            "fan_id": fan_id,
            "count": remaining.min(100),
            "older_than_token": last_token,
        });

        let resp = client
            .post("https://bandcamp.com/api/fancollection/1/collection_items")
            .json(&payload)
            .send().await.map_err(|e| format!("pagination: {e}"))?;

        let page: serde_json::Value = resp.json().await.map_err(|e| format!("parse: {e}"))?;
        let page_urls = &page["redownload_urls"];
        let mut page_count = 0;

        if let Some(page_items) = page["items"].as_array() {
            for item in page_items {
                if let Some(ci) = parse_collection_item(item, page_urls) {
                    items.push(ci);
                    page_count += 1;
                }
            }
        }

        if let Some(lt) = page["last_token"].as_str() { last_token = lt.to_string(); }
        remaining = remaining.saturating_sub(page_count.max(1));

        let _ = app.emit("collection-loading", serde_json::json!({ "loaded": items.len(), "total": total_items }));
        if page_count == 0 { break; }
    }

    Ok(CollectionResult { items, fan_id, username, collection_count })
}

pub fn parse_pagedata(html: &str) -> serde_json::Value {
    if let Some(start) = html.find("id=\"pagedata\"") {
        if let Some(blob_start) = html[start..].find("data-blob=\"") {
            let abs_start = start + blob_start + 11;
            if let Some(blob_end) = html[abs_start..].find('"') {
                let blob = &html[abs_start..abs_start + blob_end];
                let decoded = blob
                    .replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
                    .replace("&quot;", "\"").replace("&#39;", "'").replace("&#x27;", "'");
                if let Ok(data) = serde_json::from_str(&decoded) { return data; }
            }
        }
    }
    serde_json::Value::Null
}

fn parse_collection_item(item: &serde_json::Value, redownload_urls: &serde_json::Value) -> Option<CollectionItem> {
    let sale_item_type = item.get("sale_item_type")?.as_str()?.to_string();
    let sale_item_id = item.get("sale_item_id")?.as_u64()?;
    let key = format!("{}{}", sale_item_type, sale_item_id);
    let redownload_url = redownload_urls.get(&key).and_then(|v| v.as_str()).unwrap_or("").to_string();
    if redownload_url.is_empty() { return None; }

    Some(CollectionItem {
        sale_item_type, sale_item_id,
        band_name: item["band_name"].as_str().unwrap_or("Unknown").to_string(),
        item_title: item["item_title"].as_str().unwrap_or("Unknown").to_string(),
        item_id: item["item_id"].as_u64().unwrap_or(0),
        item_url: item["item_url"].as_str().unwrap_or("").to_string(),
        redownload_url,
        purchased: item["purchased"].as_str().map(|s| s.to_string()),
        item_art_id: item["item_art_id"].as_u64(),
        tralbum_type: item["tralbum_type"].as_str().map(|s| s.to_string()),
    })
}
