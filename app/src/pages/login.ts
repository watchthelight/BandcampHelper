import { tryRestoreSession, login, checkAuth } from "../lib/tauri.js";
import { switchPage } from "../main.js";

const { listen } = (window as any).__TAURI__.event;

let logEl: HTMLElement;

function log(msg: string, cls: string = ""): void {
  const line = document.createElement("div");
  line.className = `log-line ${cls}`;
  line.innerHTML = `<span class="prefix">&gt;</span> ${msg}`;
  logEl.appendChild(line);
  logEl.scrollTop = logEl.scrollHeight;
}

export async function init(el: HTMLElement): Promise<void> {
  el.innerHTML = `
    <div class="header">
      <h1>bandcamp helper</h1>
      <div class="sub">download your bandcamp collection</div>
    </div>

    <div class="status-section">
      <div class="status-header">authenticate</div>
      <div class="status-row">
        <span class="status-label">username</span>
        <input id="login-username" class="input" type="text" placeholder="your bandcamp username" style="flex:1; max-width: 280px;" />
      </div>
    </div>

    <div class="actions">
      <button id="btn-login" class="btn primary">log in to bandcamp</button>
    </div>

    <div class="log-section">
      <div class="log-header">log</div>
      <div id="login-log" class="log">
        <div class="log-line"><span class="prefix">&gt;</span> enter your username and click log in</div>
      </div>
    </div>
  `;

  logEl = document.getElementById("login-log")!;
  const usernameInput = document.getElementById("login-username") as HTMLInputElement;

  listen("login-status", (event: any) => {
    log(event.payload as string, "info");
  });

  document.getElementById("btn-login")?.addEventListener("click", async () => {
    const username = usernameInput.value.trim();
    if (!username) {
      log("enter a username first", "error");
      return;
    }

    const btn = document.getElementById("btn-login")!;
    btn.classList.add("loading");
    btn.setAttribute("disabled", "true");

    try {
      // Opens WebView to bandcamp.com/login, waits for auth, extracts cookies
      const cookieCount = await login();
      log(`extracted ${cookieCount} cookies`, "success");

      log("verifying authentication...");
      const auth = await checkAuth(username);

      if (auth.authenticated) {
        log(`authenticated as ${auth.username} (${auth.collection_count} albums)`, "success");
        (window as any).__BANDCAMP_USERNAME__ = username;
        localStorage.setItem("bc_username", username);
        setTimeout(() => switchPage("library"), 500);
      } else {
        log("cookie extraction succeeded but auth check failed — wrong username?", "error");
      }
    } catch (e: any) {
      log(`error: ${e}`, "error");
    } finally {
      btn.classList.remove("loading");
      btn.removeAttribute("disabled");
    }
  });

  // Auto-restore: try saved session cookies on page load
  const savedUsername = localStorage.getItem("bc_username");
  if (savedUsername) {
    usernameInput.value = savedUsername;
    log("restoring saved session...");
    try {
      const count = await tryRestoreSession();
      if (count > 0) {
        log(`restored ${count} cookies`, "success");
        log("verifying...");
        const auth = await checkAuth(savedUsername);
        if (auth.authenticated) {
          log(`welcome back, ${auth.username} (${auth.collection_count} albums)`, "success");
          (window as any).__BANDCAMP_USERNAME__ = savedUsername;
          setTimeout(() => switchPage("library"), 300);
          return;
        }
      }
    } catch {
      // No saved session or expired — user will log in manually
    }
    log("saved session expired — log in again", "warn");
  }
}
