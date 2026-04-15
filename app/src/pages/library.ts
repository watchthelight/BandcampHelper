import { fetchCollection, checkLocalAlbums, getDefaultOutputDirectory, type CollectionItem } from "../lib/tauri.js";
import { switchPage } from "../main.js";

let items: CollectionItem[] = [];
let selected = new Set<string>();
let localAlbums = new Set<string>();
let sortKey: "artist" | "title" | "date" = "artist";
let sortAsc = true;

function albumKey(item: CollectionItem): string {
  return `${item.sale_item_type}${item.sale_item_id}`;
}

function sortItems(list: CollectionItem[]): CollectionItem[] {
  const sorted = [...list];
  sorted.sort((a, b) => {
    let cmp = 0;
    switch (sortKey) {
      case "artist":
        cmp = a.band_name.localeCompare(b.band_name) || a.item_title.localeCompare(b.item_title);
        break;
      case "title":
        cmp = a.item_title.localeCompare(b.item_title);
        break;
      case "date":
        cmp = (b.purchased || "").localeCompare(a.purchased || "");
        break;
    }
    return sortAsc ? cmp : -cmp;
  });
  return sorted;
}

function renderList(listEl: HTMLElement): void {
  listEl.innerHTML = "";
  const sorted = sortItems(items);
  for (const item of sorted) {
    const key = albumKey(item);
    const row = document.createElement("div");
    row.className = `album-row${selected.has(key) ? " selected" : ""}`;
    row.dataset.key = key;

    const purchased = item.purchased
      ? item.purchased.replace(/ \d{2}:\d{2}:\d{2} GMT$/, "")
      : "";

    const localBadge = localAlbums.has(key) ? `<span class="local-badge">local</span>` : "";

    row.innerHTML = `
      <span class="cb"></span>
      <span class="artist">${escHtml(item.band_name)}</span>
      <span class="title">${escHtml(item.item_title)}${localBadge}</span>
      <span class="date">${purchased}</span>
    `;

    row.addEventListener("click", () => {
      if (selected.has(key)) {
        selected.delete(key);
        row.classList.remove("selected");
      } else {
        selected.add(key);
        row.classList.add("selected");
      }
      updateCount();
    });

    listEl.appendChild(row);
  }
}

function updateCount(): void {
  const btn = document.getElementById("btn-download") as HTMLButtonElement | null;
  if (btn) {
    btn.textContent = `download selected (${selected.size})`;
    btn.disabled = selected.size === 0;
  }
}

function escHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function setSortBtn(key: string): void {
  for (const btn of document.querySelectorAll<HTMLElement>(".sort-btn")) {
    btn.classList.toggle("active", btn.dataset.sort === key);
  }
}

function showConfirmPopup(localCount: number): Promise<"skip" | "redownload" | "cancel"> {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "popup-overlay";
    overlay.innerHTML = `
      <div class="popup">
        <div class="popup-text">${localCount} album${localCount > 1 ? "s" : ""} already downloaded locally</div>
        <div class="popup-sub">choose how to handle existing albums</div>
        <div class="popup-actions">
          <button class="btn" data-action="cancel">cancel</button>
          <button class="btn warn" data-action="skip">skip local</button>
          <button class="btn primary" data-action="redownload">re-download anyways</button>
        </div>
      </div>
    `;

    const handle = (action: "skip" | "redownload" | "cancel") => {
      overlay.remove();
      resolve(action);
    };

    for (const btn of overlay.querySelectorAll<HTMLElement>("[data-action]")) {
      btn.addEventListener("click", () => handle(btn.dataset.action as any));
    }

    document.body.appendChild(overlay);
  });
}

export async function init(el: HTMLElement): Promise<void> {
  el.innerHTML = `
    <div class="header">
      <h1 id="lib-header">loading collection...</h1>
    </div>

    <div class="toolbar">
      <button id="btn-select-all" class="btn">select all</button>
      <button id="btn-deselect-all" class="btn">deselect all</button>
      <div class="spacer"></div>
      <div class="sort-group">
        <span style="color: var(--dim); font-size: 11px; text-transform: uppercase; letter-spacing: 0.08em;">sort</span>
        <button class="sort-btn active" data-sort="artist">artist</button>
        <button class="sort-btn" data-sort="title">title</button>
        <button class="sort-btn" data-sort="date">date</button>
      </div>
      <div class="spacer"></div>
      <label style="color: var(--dim); font-size: 12px;">format</label>
      <select id="format-select" class="select">
        <option value="flac">flac</option>
        <option value="alac">alac</option>
        <option value="mp3-320">mp3-320</option>
        <option value="mp3-v0">mp3-v0</option>
        <option value="aac-hi">aac-hi</option>
        <option value="aiff-lossless">aiff-lossless</option>
        <option value="vorbis">vorbis</option>
        <option value="wav">wav</option>
      </select>
      <button id="btn-download" class="btn primary" disabled>download selected (0)</button>
    </div>

    <div id="album-list" class="album-list"></div>
  `;

  const listEl = document.getElementById("album-list")!;
  const headerEl = document.getElementById("lib-header")!;
  const username = (window as any).__BANDCAMP_USERNAME__ || "";

  try {
    const result = await fetchCollection(username);
    items = result.items;
    headerEl.textContent = `your collection (${items.length} albums)`;

    // Check local albums
    const format = (document.getElementById("format-select") as HTMLSelectElement).value;
    const outputDir = await getDefaultOutputDirectory();
    try {
      const locals = await checkLocalAlbums({ items, format, output_dir: outputDir });
      localAlbums = new Set(locals);
    } catch { /* ignore — just won't show badges */ }

    renderList(listEl);
  } catch (e: any) {
    headerEl.textContent = "failed to load collection";
    listEl.innerHTML = `<div class="log-line error" style="padding:10px;">${e}</div>`;
  }

  // Re-check local albums when format changes
  document.getElementById("format-select")?.addEventListener("change", async () => {
    const format = (document.getElementById("format-select") as HTMLSelectElement).value;
    const outputDir = await getDefaultOutputDirectory();
    try {
      const locals = await checkLocalAlbums({ items, format, output_dir: outputDir });
      localAlbums = new Set(locals);
    } catch { localAlbums.clear(); }
    renderList(listEl);
  });

  // Sort buttons
  for (const btn of document.querySelectorAll<HTMLElement>(".sort-btn")) {
    btn.addEventListener("click", () => {
      const key = btn.dataset.sort as typeof sortKey;
      if (sortKey === key) {
        sortAsc = !sortAsc;
      } else {
        sortKey = key;
        sortAsc = true;
      }
      setSortBtn(key);
      renderList(listEl);
    });
  }

  document.getElementById("btn-select-all")?.addEventListener("click", () => {
    selected = new Set(items.map(albumKey));
    for (const card of listEl.querySelectorAll<HTMLElement>(".album-row")) {
      card.classList.add("selected");
    }
    updateCount();
  });

  document.getElementById("btn-deselect-all")?.addEventListener("click", () => {
    selected.clear();
    for (const card of listEl.querySelectorAll<HTMLElement>(".album-row")) {
      card.classList.remove("selected");
    }
    updateCount();
  });

  document.getElementById("btn-download")?.addEventListener("click", async () => {
    const selectedItems = items.filter(i => selected.has(albumKey(i)));
    const format = (document.getElementById("format-select") as HTMLSelectElement).value;

    // Check how many selected albums are local
    const localSelected = selectedItems.filter(i => localAlbums.has(albumKey(i)));

    if (localSelected.length > 0) {
      const action = await showConfirmPopup(localSelected.length);
      if (action === "cancel") return;

      if (action === "skip") {
        const filtered = selectedItems.filter(i => !localAlbums.has(albumKey(i)));
        if (filtered.length === 0) return;
        (window as any).__DOWNLOAD_REQUEST__ = { items: filtered, format, force: false };
      } else {
        // redownload
        (window as any).__DOWNLOAD_REQUEST__ = { items: selectedItems, format, force: true };
      }
    } else {
      (window as any).__DOWNLOAD_REQUEST__ = { items: selectedItems, format, force: false };
    }

    switchPage("downloads");
  });
}
