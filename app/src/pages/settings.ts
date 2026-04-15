import { getDefaultOutputDirectory } from "../lib/tauri.js";

export async function init(el: HTMLElement): Promise<void> {
  const defaultDir = await getDefaultOutputDirectory();

  el.innerHTML = `
    <div class="header">
      <h1>settings</h1>
    </div>

    <div class="status-section">
      <div class="status-header">output</div>
      <div class="status-row">
        <span class="status-label">directory</span>
        <span id="settings-dir" class="status-value">${defaultDir}</span>
        <button id="btn-browse" class="btn" style="padding: 4px 12px; font-size: 12px;">browse</button>
      </div>
    </div>

    <div class="status-section">
      <div class="status-header">downloads</div>
      <div class="status-row">
        <span class="status-label">parallel</span>
        <input id="settings-parallel" type="range" min="1" max="8" value="4" style="flex: 1; max-width: 200px; accent-color: var(--green);" />
        <span id="settings-parallel-val" class="status-value">4</span>
      </div>
      <div class="status-row">
        <span class="status-label">auto-extract</span>
        <label style="display: flex; align-items: center; gap: 6px; cursor: pointer;">
          <input id="settings-extract" type="checkbox" checked style="accent-color: var(--green);" />
          <span class="status-value">extract .zip archives after download</span>
        </label>
      </div>
    </div>

    <div class="status-section">
      <div class="status-header">about</div>
      <div class="status-row">
        <span class="status-label">version</span>
        <span class="status-value">1.0.0</span>
      </div>
    </div>
  `;

  const parallelSlider = document.getElementById("settings-parallel") as HTMLInputElement;
  const parallelVal = document.getElementById("settings-parallel-val")!;
  parallelSlider.addEventListener("input", () => {
    parallelVal.textContent = parallelSlider.value;
  });

  document.getElementById("btn-browse")?.addEventListener("click", () => {
    // TODO: implement folder picker
    console.log("browse not yet implemented");
  });
}
