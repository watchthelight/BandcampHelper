# BandcampHelper

Desktop app for downloading your Bandcamp collection. Tauri, Rust, TypeScript.

Log in through Bandcamp's own login page inside the app, and it pulls your full collection. Pick a format, pick which albums, hit download. Parallel downloads, zip extraction, resume support.

## What it does

You log in to Bandcamp through a built-in browser window. The app grabs your session cookies, fetches your collection (paginated, so it gets everything), and shows your albums in a grid. Select what you want, pick a format (FLAC, MP3-320, AAC, WAV, whatever Bandcamp offers), and download.

Downloads run in parallel (configurable, 1 to 8). Progress bars update in real time. If a download is a zip, the app can extract it automatically and clean up the archive. Files that already exist on disk get skipped, so re-running after an interruption picks up where it left off.

Albums land in `{output dir}/{artist}/{artist} - {title}.{ext}`.

## Building from source

Rust, Node.js 20+, pnpm.

```
cd app
pnpm install
pnpm tauri dev
pnpm tauri build
```

Installer output goes to `app/src-tauri/target/release/bundle/`.

## How it works

The Rust backend handles all the heavy lifting. Authentication works by opening a WebView to `bandcamp.com/login`, waiting for the redirect, then extracting cookies from the WebView2 SQLite database (DPAPI + AES-256-GCM decryption on Windows). Collection data comes from Bandcamp's `pagedata` JSON blob and the `/api/fancollection/1/collection_items` endpoint for pagination.

Downloads stream through reqwest with progress events emitted to the frontend every 100ms. Sleep prevention keeps your machine awake during long downloads.

## Layout

```
app/
  src/              # TypeScript frontend (vanilla, no framework)
  src-tauri/src/    # Rust backend
    auth.rs         # WebView login, cookie extraction, collection fetch
    download.rs     # Streaming downloads, zip extraction, progress
    models.rs       # Shared data structures
```

## License

MIT
