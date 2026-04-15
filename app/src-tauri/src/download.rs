use crate::models::{CheckLocalRequest, CollectionItem, DownloadProgress, DownloadRequest, OverallProgress};
use crate::BandcampState;
use futures::StreamExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Semaphore;

fn format_extension(format: &str, is_album: bool) -> &'static str {
    if is_album { return ".zip"; }
    match format {
        "aac-hi" => ".m4a",
        "aiff-lossless" => ".aiff",
        "alac" => ".m4a",
        "flac" => ".flac",
        "mp3-320" => ".mp3",
        "mp3-v0" => ".mp3",
        "vorbis" => ".ogg",
        "wav" => ".wav",
        _ => ".zip",
    }
}

fn sanitize_path(s: &str) -> String {
    sanitize_filename::sanitize_with_options(s, sanitize_filename::Options {
        replacement: "-",
        ..Default::default()
    })
}

fn album_key(item: &CollectionItem) -> String {
    format!("{}{}", item.sale_item_type, item.sale_item_id)
}

fn item_file_path(item: &CollectionItem, format: &str, output_dir: &str, force_album: bool) -> PathBuf {
    let is_album = if force_album { true } else {
        item.tralbum_type.as_deref().map(|t| t == "a").unwrap_or(true)
    };
    let ext = format_extension(format, is_album);
    let artist_dir = sanitize_path(&item.band_name);
    let filename = format!("{} - {}{}", sanitize_path(&item.band_name), sanitize_path(&item.item_title), ext);
    PathBuf::from(output_dir).join(&artist_dir).join(&filename)
}

const AUDIO_EXTS: &[&str] = &["flac", "mp3", "m4a", "ogg", "wav", "aiff", "aif", "opus"];

fn dir_has_audio_files(dir: &Path) -> bool {
    std::fs::read_dir(dir).map(|entries| {
        entries.filter_map(|e| e.ok()).any(|e| {
            e.path().extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| AUDIO_EXTS.contains(&ext.to_lowercase().as_str()))
                .unwrap_or(false)
        })
    }).unwrap_or(false)
}

/// Check which items already exist on disk.
/// Albums: zip exists OR extracted dir with audio files inside.
/// Tracks: individual file exists.
/// Unknown type: checks both album and track paths.
#[tauri::command]
pub fn check_local_albums(request: CheckLocalRequest) -> Vec<String> {
    request.items.iter().filter(|item| {
        let tralbum = item.tralbum_type.as_deref();
        match tralbum {
            Some("t") => {
                // Track — check single file
                let path = item_file_path(item, &request.format, &request.output_dir, false);
                path.exists()
            }
            Some("a") => {
                // Album — check zip OR extracted dir with audio files
                let path = item_file_path(item, &request.format, &request.output_dir, true);
                path.exists() || {
                    let dir = path.with_extension("");
                    dir.is_dir() && dir_has_audio_files(&dir)
                }
            }
            _ => {
                // Unknown — try both
                let album_path = item_file_path(item, &request.format, &request.output_dir, true);
                let track_path = item_file_path(item, &request.format, &request.output_dir, false);
                track_path.exists() || album_path.exists() || {
                    let dir = album_path.with_extension("");
                    dir.is_dir() && dir_has_audio_files(&dir)
                }
            }
        }
    }).map(|item| album_key(item)).collect()
}

#[tauri::command]
pub async fn start_downloads(
    request: DownloadRequest,
    state: tauri::State<'_, BandcampState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let client = state.client.lock().await;
    let client = client.as_ref().ok_or("not authenticated")?.clone();

    state.cancel_flag.store(false, Ordering::SeqCst);
    let cancel_flag = state.cancel_flag.clone();

    std::fs::create_dir_all(&request.output_dir)
        .map_err(|e| format!("cannot create output dir: {e}"))?;

    #[cfg(target_os = "windows")]
    set_prevent_sleep(true);

    let sem = Arc::new(Semaphore::new(request.parallel.clamp(1, 8)));
    let total = request.items.len();
    let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let failed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let skipped = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let mut handles = Vec::new();

    for item in request.items.iter().cloned() {
        let sem = sem.clone();
        let client = client.clone();
        let app = app.clone();
        let format = request.format.clone();
        let output_dir = request.output_dir.clone();
        let extract = request.extract;
        let force = request.force;
        let cancel = cancel_flag.clone();
        let completed = completed.clone();
        let failed = failed.clone();
        let skipped = skipped.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let key = album_key(&item);

            if cancel.load(Ordering::SeqCst) { return; }

            let _ = app.emit("download-status", DownloadProgress {
                album_key: key.clone(),
                artist: item.band_name.clone(),
                title: item.item_title.clone(),
                status: "Downloading".to_string(),
                bytes_downloaded: 0, bytes_total: 0,
                file_path: String::new(), error: None,
            });

            match download_single_album(&client, &item, &format, &output_dir, extract, force, &app, &cancel).await {
                Ok(status) => {
                    match status.as_str() {
                        "Done" => { completed.fetch_add(1, Ordering::SeqCst); }
                        "Skipped" => { skipped.fetch_add(1, Ordering::SeqCst); }
                        _ => { failed.fetch_add(1, Ordering::SeqCst); }
                    }
                    let _ = app.emit("download-status", DownloadProgress {
                        album_key: key.clone(),
                        artist: item.band_name.clone(),
                        title: item.item_title.clone(),
                        status,
                        bytes_downloaded: 0, bytes_total: 0,
                        file_path: String::new(), error: None,
                    });
                }
                Err(e) => {
                    failed.fetch_add(1, Ordering::SeqCst);
                    let _ = app.emit("download-status", DownloadProgress {
                        album_key: key.clone(),
                        artist: item.band_name.clone(),
                        title: item.item_title.clone(),
                        status: "Error".to_string(),
                        bytes_downloaded: 0, bytes_total: 0,
                        file_path: String::new(), error: Some(e),
                    });
                }
            }

            let _ = app.emit("download-overall", OverallProgress {
                total,
                completed: completed.load(Ordering::SeqCst),
                failed: failed.load(Ordering::SeqCst),
                skipped: skipped.load(Ordering::SeqCst),
            });

            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        });

        handles.push(handle);
    }

    for handle in handles { let _ = handle.await; }

    #[cfg(target_os = "windows")]
    set_prevent_sleep(false);

    Ok(())
}

async fn download_single_album(
    client: &reqwest::Client,
    item: &CollectionItem,
    format: &str,
    output_dir: &str,
    extract: bool,
    force: bool,
    app: &tauri::AppHandle,
    cancel: &Arc<std::sync::atomic::AtomicBool>,
) -> Result<String, String> {
    let key = album_key(item);

    // Fetch redownload page to get actual download URL
    let resp = client.get(&item.redownload_url).send().await
        .map_err(|e| format!("fetch redownload page: {e}"))?;
    let text = resp.text().await.map_err(|e| format!("read page: {e}"))?;

    // Get the stat URL from pagedata, then call it to get the real CDN download URL
    let download_url = {
        let data = crate::auth::parse_pagedata(&text);
        let download_items = data.get("download_items")
            .and_then(|v| v.as_array())
            .ok_or("no download_items")?;

        if download_items.is_empty() {
            return Err("no download items available".to_string());
        }

        let dl_item = &download_items[0];
        let downloads = dl_item.get("downloads")
            .ok_or("no downloads available for this album")?;
        let format_info = downloads.get(format)
            .ok_or(format!("format '{format}' not available"))?;

        // The download URL — use directly (like the Python downloader does)
        let mut dl_url = format_info.get("url").and_then(|v| v.as_str())
            .ok_or("no download URL")?.to_string();

        // Ensure HTTPS
        if dl_url.starts_with("http://") {
            dl_url = dl_url.replace("http://", "https://");
        }

        eprintln!("[dl] download URL: {}...", &dl_url[..dl_url.len().min(80)]);
        dl_url
    };

    let is_album = item.tralbum_type.as_deref().map(|t| t == "a").unwrap_or(true);
    let ext = format_extension(format, is_album);
    let artist_dir = sanitize_path(&item.band_name);
    let filename = format!("{} - {}{}", sanitize_path(&item.band_name), sanitize_path(&item.item_title), ext);
    let file_path = PathBuf::from(output_dir).join(&artist_dir).join(&filename);

    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
    }

    if !force && file_path.exists() { return Ok("Skipped".to_string()); }
    if cancel.load(Ordering::SeqCst) { return Err("cancelled".to_string()); }

    eprintln!("[dl] fetching: {}...", &download_url[..download_url.len().min(80)]);
    // Log cookies being sent
    if let Some(cookie_header) = client.get(&download_url).build().ok()
        .and_then(|r| r.headers().get("cookie").map(|v| v.to_str().unwrap_or("?").to_string())) {
        eprintln!("[dl] cookies header: {}...", &cookie_header[..cookie_header.len().min(60)]);
    } else {
        eprintln!("[dl] no cookie header on request");
    }
    let resp = client.get(&download_url).send().await
        .map_err(|e| format!("download request: {e}"))?;

    let status = resp.status();
    let content_type = resp.headers().get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    eprintln!("[dl] status={}, content-type={}", status, content_type);

    if status == reqwest::StatusCode::FORBIDDEN { return Err("HTTP 403".to_string()); }
    if !status.is_success() { return Err(format!("HTTP {status}")); }

    // If Bandcamp returned HTML instead of a file, the URL was wrong or needs auth
    if content_type.contains("text/html") {
        return Err("download returned HTML instead of file — auth may have expired".to_string());
    }

    let total_size = resp.content_length().unwrap_or(0);
    let mut stream = resp.bytes_stream();
    let mut file = tokio::fs::File::create(&file_path).await
        .map_err(|e| format!("create file: {e}"))?;

    let mut downloaded: u64 = 0;
    let mut last_emit = std::time::Instant::now();
    use tokio::io::AsyncWriteExt;

    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::SeqCst) {
            drop(file);
            let _ = tokio::fs::remove_file(&file_path).await;
            return Err("cancelled".to_string());
        }
        let chunk = chunk.map_err(|e| format!("stream: {e}"))?;
        file.write_all(&chunk).await.map_err(|e| format!("write: {e}"))?;
        downloaded += chunk.len() as u64;

        if last_emit.elapsed() >= std::time::Duration::from_millis(100) {
            let _ = app.emit("download-progress", DownloadProgress {
                album_key: key.clone(),
                artist: item.band_name.clone(),
                title: item.item_title.clone(),
                status: "Downloading".to_string(),
                bytes_downloaded: downloaded,
                bytes_total: total_size,
                file_path: file_path.display().to_string(),
                error: None,
            });
            last_emit = std::time::Instant::now();
        }
    }

    file.flush().await.map_err(|e| format!("flush: {e}"))?;
    drop(file);

    // Only extract if file starts with ZIP magic bytes (PK\x03\x04)
    let is_zip_file = std::fs::File::open(&file_path)
        .and_then(|mut f| {
            use std::io::Read;
            let mut magic = [0u8; 4];
            f.read_exact(&mut magic)?;
            Ok(magic == [0x50, 0x4B, 0x03, 0x04])
        })
        .unwrap_or(false);

    if extract && ext == ".zip" && is_zip_file {
        let _ = app.emit("download-status", DownloadProgress {
            album_key: key.clone(),
            artist: item.band_name.clone(),
            title: item.item_title.clone(),
            status: "Extracting".to_string(),
            bytes_downloaded: downloaded, bytes_total: total_size,
            file_path: file_path.display().to_string(), error: None,
        });
        let extract_dir = file_path.with_extension("");
        extract_zip(&file_path, &extract_dir)?;
        let _ = std::fs::remove_file(&file_path);
    }

    Ok("Done".to_string())
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("open zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("read zip: {e}"))?;
    std::fs::create_dir_all(dest).map_err(|e| format!("create extract dir: {e}"))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("zip entry: {e}"))?;
        let outpath = dest.join(entry.enclosed_name().ok_or("invalid zip entry name")?);
        if entry.is_dir() {
            std::fs::create_dir_all(&outpath).map_err(|e| format!("create dir: {e}"))?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("create parent: {e}"))?;
            }
            let mut outfile = std::fs::File::create(&outpath).map_err(|e| format!("create file: {e}"))?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| format!("extract: {e}"))?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_downloads(state: tauri::State<'_, BandcampState>) -> Result<(), String> {
    state.cancel_flag.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn get_default_output_directory() -> String {
    let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join("Music").join("Bandcamp").display().to_string()
}

#[cfg(target_os = "windows")]
fn set_prevent_sleep(enable: bool) {
    use windows_sys::Win32::System::Power::SetThreadExecutionState;
    use windows_sys::Win32::System::Power::{ES_CONTINUOUS, ES_SYSTEM_REQUIRED};
    unsafe {
        if enable {
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
        } else {
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn set_prevent_sleep(_enable: bool) {}
