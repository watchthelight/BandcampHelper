const { invoke } = (window as any).__TAURI__.core;

export interface AuthStatus {
  authenticated: boolean;
  username: string;
  collection_count: number;
}

export interface CollectionItem {
  sale_item_type: string;
  sale_item_id: number;
  band_name: string;
  item_title: string;
  item_id: number;
  item_url: string;
  redownload_url: string;
  purchased: string | null;
  item_art_id: number | null;
  tralbum_type: string | null;
}

export interface CollectionResult {
  items: CollectionItem[];
  fan_id: number;
  username: string;
  collection_count: number;
}

export interface DownloadRequest {
  items: CollectionItem[];
  format: string;
  output_dir: string;
  parallel: number;
  extract: boolean;
  force?: boolean;
}

export interface CheckLocalRequest {
  items: CollectionItem[];
  format: string;
  output_dir: string;
}

export interface DownloadProgress {
  album_key: string;
  artist: string;
  title: string;
  status: string;
  bytes_downloaded: number;
  bytes_total: number;
  file_path: string;
  error: string | null;
}

export interface OverallProgress {
  total: number;
  completed: number;
  failed: number;
  skipped: number;
}

export async function tryRestoreSession(): Promise<number> {
  return invoke("try_restore_session");
}

export async function login(): Promise<number> {
  return invoke("login");
}

export async function checkAuth(username: string): Promise<AuthStatus> {
  return invoke("check_auth", { username });
}

export async function fetchCollection(username: string): Promise<CollectionResult> {
  return invoke("fetch_collection", { username });
}

export async function startDownloads(request: DownloadRequest): Promise<void> {
  return invoke("start_downloads", { request });
}

export async function cancelDownloads(): Promise<void> {
  return invoke("cancel_downloads");
}

export async function getDefaultOutputDirectory(): Promise<string> {
  return invoke("get_default_output_directory");
}

export async function checkLocalAlbums(request: CheckLocalRequest): Promise<string[]> {
  return invoke("check_local_albums", { request });
}
