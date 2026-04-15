import { startDownloads, cancelDownloads, getDefaultOutputDirectory, type DownloadRequest, type DownloadProgress, type OverallProgress } from "../lib/tauri.js";
import { onDownloadProgress, onDownloadStatus, onOverallProgress } from "../lib/events.js";

let logEl: HTMLElement;
let listEl: HTMLElement;
let fillEl: HTMLElement;
let summaryEl: HTMLElement;
let downloading = false;

function log(msg: string, cls: string = ""): void {
  const line = document.createElement("div");
  line.className = `log-line ${cls}`;
  line.innerHTML = `<span class="prefix">&gt;</span> ${msg}`;
  logEl.appendChild(line);
  logEl.scrollTop = logEl.scrollHeight;
}

function formatBytes(b: number): string {
  if (b < 1024) return `${b} B`;
  if (b < 1048576) return `${(b / 1024).toFixed(1)} KB`;
  if (b < 1073741824) return `${(b / 1048576).toFixed(1)} MB`;
  return `${(b / 1073741824).toFixed(2)} GB`;
}

function updateRow(p: DownloadProgress): void {
  let row = listEl.querySelector(`[data-key="${p.album_key}"]`) as HTMLElement | null;
  if (!row) {
    row = document.createElement("div");
    row.className = "download-row";
    row.dataset.key = p.album_key;
    row.innerHTML = `
      <span class="status-dot"></span>
      <span class="dl-artist"></span>
      <span class="dl-title"></span>
      <span class="dl-status"></span>
    `;
    listEl.appendChild(row);
  }

  const dot = row.querySelector(".status-dot")!;
  const artist = row.querySelector(".dl-artist")!;
  const title = row.querySelector(".dl-title")!;
  const status = row.querySelector(".dl-status")!;

  artist.textContent = p.artist;
  title.textContent = p.title;
  dot.className = "status-dot";
  status.className = "dl-status";

  switch (p.status) {
    case "Downloading":
      dot.classList.add("yellow");
      status.classList.add("active");
      status.textContent = p.bytes_total > 0
        ? `${formatBytes(p.bytes_downloaded)} / ${formatBytes(p.bytes_total)}`
        : "downloading...";
      break;
    case "Done":
      dot.classList.add("green");
      status.classList.add("done");
      status.textContent = "done";
      break;
    case "Error":
      dot.classList.add("red");
      status.classList.add("error");
      status.textContent = p.error || "error";
      break;
    case "Extracting":
      dot.classList.add("yellow");
      status.classList.add("active");
      status.textContent = "extracting...";
      break;
    case "Skipped":
      dot.classList.add("green");
      status.classList.add("done");
      status.textContent = "skipped";
      break;
    default:
      status.classList.add("queued");
      status.textContent = "queued";
  }
}

export async function init(el: HTMLElement): Promise<void> {
  el.innerHTML = `
    <div class="header">
      <h1 id="dl-summary">preparing downloads...</h1>
    </div>
    <div class="progress-bar"><div id="dl-fill" class="fill" style="width: 0%"></div></div>
    <div class="actions">
      <button id="btn-cancel" class="btn danger" disabled>cancel</button>
    </div>
    <div id="download-list" class="download-list"></div>
    <div class="log-section" style="max-height: 160px;">
      <div class="log-header">log</div>
      <div id="dl-log" class="log"></div>
    </div>
  `;

  logEl = document.getElementById("dl-log")!;
  listEl = document.getElementById("download-list")!;
  fillEl = document.getElementById("dl-fill")!;
  summaryEl = document.getElementById("dl-summary")!;
  const cancelBtn = document.getElementById("btn-cancel") as HTMLButtonElement;

  onDownloadProgress((p) => updateRow(p));
  onDownloadStatus((p) => {
    updateRow(p);
    if (p.status === "Downloading") log(`downloading: ${p.artist} - ${p.title}`);
    else if (p.status === "Done") log(`done: ${p.artist} - ${p.title}`, "success");
    else if (p.status === "Error") log(`error: ${p.artist} - ${p.title}: ${p.error}`, "error");
    else if (p.status === "Extracting") log(`extracting: ${p.artist} - ${p.title}`);
  });

  onOverallProgress((o) => {
    const done = o.completed + o.failed + o.skipped;
    const pct = o.total > 0 ? Math.round((done / o.total) * 100) : 0;
    fillEl.style.width = `${pct}%`;
    summaryEl.textContent = `${done} of ${o.total} completed`;
    if (done >= o.total && downloading) {
      downloading = false;
      cancelBtn.disabled = true;
      log(`finished: ${o.completed} downloaded, ${o.failed} failed, ${o.skipped} skipped`,
        o.failed > 0 ? "warn" : "success");
    }
  });

  cancelBtn.addEventListener("click", async () => {
    try {
      await cancelDownloads();
      log("downloads cancelled", "warn");
      downloading = false;
      cancelBtn.disabled = true;
    } catch (e: any) {
      log(`cancel error: ${e}`, "error");
    }
  });

  const req = (window as any).__DOWNLOAD_REQUEST__;
  if (!req || req.items.length === 0) {
    summaryEl.textContent = "no downloads queued";
    log("select albums from the library tab, then click download", "info");
    return;
  }

  downloading = true;
  cancelBtn.disabled = false;

  const outputDir = await getDefaultOutputDirectory();
  const request: DownloadRequest = {
    items: req.items,
    format: req.format,
    output_dir: outputDir,
    parallel: 4,
    extract: true,
    force: req.force || false,
  };

  log(`starting ${request.items.length} downloads (${request.format})`, "info");
  summaryEl.textContent = `0 of ${request.items.length} completed`;

  for (const item of request.items) {
    updateRow({
      album_key: `${item.sale_item_type}${item.sale_item_id}`,
      artist: item.band_name,
      title: item.item_title,
      status: "Queued",
      bytes_downloaded: 0, bytes_total: 0,
      file_path: "", error: null,
    });
  }

  try {
    await startDownloads(request);
  } catch (e: any) {
    log(`download error: ${e}`, "error");
    downloading = false;
    cancelBtn.disabled = true;
  }
}
