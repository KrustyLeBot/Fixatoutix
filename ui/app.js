// Fixatoutix flyout panel. Vanilla JS, no framework: the state lives in Rust, this only draws it.
(() => {
  "use strict";
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;
  const root = document.getElementById("app");

  let S = null; // state from Rust (get_state)
  let t = window.makeT("fr");
  const ui = {
    view: "main", // main | picker | settings
    menu: null, // rule id whose "..." menu is open
    sheet: null, // { type: "rename" | "match", id, error? }
    windows: [],
    updating: null, // null or percent
    updateError: "",
    capture: false,
    shortcutError: "",
    checkMsg: "",
    checkErr: false,
  };

  const esc = (s) =>
    String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

  // Fluent-style stroke icons.
  const svg = (d) =>
    `<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${d}</svg>`;
  const I = {
    nail: svg('<path d="M11 9l-7 7"/><path d="M8.2 6.2l5.6 5.6" stroke-width="3"/><path d="M13 4.5l2.5-1 1 2.5" opacity=".0"/>'),
    gather: svg('<path d="M3 8V3h5M17 8V3h-5M3 12v5h5M17 12v5h-5"/><rect x="7" y="7" width="6" height="6" rx="1"/>'),
    gear: svg('<circle cx="10" cy="10" r="2.5"/><path d="M10 2.5v2M10 15.5v2M2.5 10h2M15.5 10h2M4.7 4.7l1.4 1.4M13.9 13.9l1.4 1.4M4.7 15.3l1.4-1.4M13.9 6.1l1.4-1.4"/>'),
    back: svg('<path d="M12 4l-6 6 6 6"/>'),
    more: svg('<circle cx="4.5" cy="10" r="1" fill="currentColor"/><circle cx="10" cy="10" r="1" fill="currentColor"/><circle cx="15.5" cy="10" r="1" fill="currentColor"/>'),
    edit: svg('<path d="M4 16l1-4 8-8 3 3-8 8z"/>'),
    pin: svg('<path d="M10 17v-5M6 12h8l-1-3V4H7v5z"/>'),
    filter: svg('<path d="M3 5h14M6 10h8M8 15h4"/>'),
    trash: svg('<path d="M4 6h12M8 6V4h4v2M6 6l1 10h6l1-10"/>'),
    folder: svg('<path d="M2.5 5.5h5l1.5 2h8v8h-14.5z"/>'),
    refresh: svg('<path d="M16 8a6 6 0 1 0 .5 4M16 3v5h-5"/>'),
    download: svg('<path d="M10 3v10M6 9l4 4 4-4M4 16h12"/>'),
  };

  async function refresh() {
    S = await invoke("get_state");
    t = window.makeT(S.settings.language);
    document.documentElement.lang = S.settings.language;
    render();
  }

  const monitorLabel = (rule) => {
    const m = /DISPLAY(\d+)/i.exec(rule.monitor?.deviceName || "");
    return m ? t("screen", { n: m[1] }) : "";
  };

  function resistText(r) {
    if (!r.monitorPresent) return t("resist.monitor");
    if (r.status === "admin") return t("resist.admin");
    if (r.status === "gaveup") return t("resist.gaveup");
    if (r.status === "error") return t("resist.error");
    return "";
  }

  function iconHtml(r) {
    return r.icon
      ? `<img class="icon" src="${esc(r.icon)}" alt="">`
      : `<span class="icon fallback" aria-hidden="true">${esc((r.label || "?").slice(0, 1).toUpperCase())}</span>`;
  }

  function ruleHtml(r) {
    const sub = [r.match.exe, monitorLabel(r), `${r.rect.w}×${r.rect.h} @ ${r.rect.x},${r.rect.y}`].filter(Boolean).join(" · ");
    const problem = resistText(r);
    const menu =
      ui.menu === r.id
        ? `<div class="menu" role="menu">
             <button role="menuitem" data-act="rename" data-id="${r.id}">${I.edit}${t("menu.rename")}</button>
             <button role="menuitem" data-act="reclou" data-id="${r.id}">${I.pin}${t("menu.reclou")}</button>
             <button role="menuitem" data-act="editMatch" data-id="${r.id}">${I.filter}${t("menu.match")}</button>
             <button role="menuitem" class="danger" data-act="del" data-id="${r.id}">${I.trash}${t("menu.delete")}</button>
           </div>`
        : "";
    return `<li class="rule ${r.enabled ? "" : "off"}" draggable="true" data-id="${r.id}">
      <div class="row1">
        ${iconHtml(r)}
        <span class="label" title="${esc(r.label)}">${esc(r.label)}</span>
        <button class="switch" role="switch" aria-checked="${r.enabled}" aria-label="${t("rule.toggle")}: ${esc(r.label)}" data-act="toggleRule" data-id="${r.id}"></button>
        <button class="icon-btn" aria-label="${t("rule.more")}" aria-haspopup="menu" data-act="more" data-id="${r.id}">${I.more}</button>
      </div>
      <div class="sub" title="${esc(sub)}">${esc(sub)}</div>
      <div class="row2">
        <div class="seg" role="group" aria-label="Mode">
          <button aria-pressed="${r.mode === "lock"}" title="${t("mode.lock.hint")}" data-act="setMode" data-id="${r.id}" data-mode="lock">${t("mode.lock")}</button>
          <button aria-pressed="${r.mode === "restore"}" title="${t("mode.restore.hint")}" data-act="setMode" data-id="${r.id}" data-mode="restore">${t("mode.restore")}</button>
        </div>
      </div>
      ${problem ? `<div class="resist" role="alert">${esc(problem)}</div>` : ""}
      ${menu}
    </li>`;
  }

  function updateBanner() {
    if (!S.update) return "";
    const busy = ui.updating !== null;
    return `<div class="banner">
      <span class="txt" id="update-text">${busy ? t("update.installing", { pct: ui.updating }) : esc(t("update.available", { build: S.update.build }))}${
        ui.updateError ? `<br><span style="color:var(--danger)">${esc(ui.updateError)}</span>` : ""
      }</span>
      <button class="btn primary" data-act="installUpdate" ${busy ? "disabled" : ""}>${I.download}${t("update.install")}</button>
    </div>`;
  }

  function viewMain() {
    const paused = S.settings.paused;
    const empty = `<div class="empty">${window.mascot(paused ? "banquet" : "guard")}
      <h2>${t("empty.title")}</h2>
      <p>${t("empty.text", { key: `<kbd>${esc(S.settings.shortcut)}</kbd>` })}</p></div>`;
    return `
      <div class="head">
        ${window.mascot(paused ? "banquet" : "guard").replace('class="mascot"', 'class="mascot" aria-hidden="true"')}
        <div class="titles">
          <h1>Fixatoutix</h1>
          <div class="state ${paused ? "paused" : ""}" title="${paused ? t("app.banquet.hint") : t("app.guard.hint")}">${paused ? t("app.banquet") : t("app.guard")}</div>
        </div>
        <button class="switch" role="switch" aria-checked="${!paused}" aria-label="${t("app.guard")} / ${t("app.banquet")}" data-act="toggleGuard"></button>
      </div>
      <div class="toolbar"><button class="btn grow" data-act="gather">${I.gather}${t("gather")}</button></div>
      ${updateBanner()}
      <main id="main">${S.rules.length ? `<ul class="rules" id="rules">${S.rules.map(ruleHtml).join("")}</ul>` : empty}</main>
      <footer>
        <button class="link" data-act="settings">${I.gear}${t("settings")}</button>
        <span class="spacer"></span>
        <button class="btn primary" data-act="openPicker">${I.nail}${t("nail")}</button>
      </footer>`;
  }

  function viewPicker() {
    const rows = ui.windows.length
      ? ui.windows
          .map(
            (w) => `<button class="pick" data-act="pick" data-hwnd="${w.hwnd}">
              ${w.icon ? `<img class="icon" style="width:28px;height:28px" src="${esc(w.icon)}" alt="">` : `<span class="icon fallback" style="width:28px;height:28px">${esc(w.label.slice(0, 1))}</span>`}
              <span class="t"><b>${esc(w.label)}</b><span>${esc(w.title)}</span></span>
              ${w.pinned ? `<span class="tag">${t("picker.pinned")}</span>` : ""}
            </button>`
          )
          .join("")
      : `<div class="note">${t("picker.empty")}</div>`;
    return `<div class="subhead"><button class="icon-btn" aria-label="${t("back")}" data-act="back">${I.back}</button><h2>${t("picker.title")}</h2></div>
      <main>${rows}</main><div style="height:10px"></div>`;
  }

  const toggle = (on, act, label) =>
    `<button class="switch" role="switch" aria-checked="${on}" aria-label="${esc(label)}" data-act="${act}"></button>`;

  function viewSettings() {
    const s = S.settings;
    const lang = (code, name) => `<button aria-pressed="${s.language === code}" data-act="setLang" data-lang="${code}">${name}</button>`;
    return `<div class="subhead"><button class="icon-btn" aria-label="${t("back")}" data-act="back">${I.back}</button><h2>${t("settings")}</h2></div>
      <main>
        <div class="group">
          <div class="setting"><div class="t">${t("s.shortcut")}<small>${t("s.shortcut.hint")}</small></div>
            <button class="btn keycap ${ui.capture ? "capturing" : ""}" data-act="captureShortcut">${ui.capture ? t("s.shortcut.press") : esc(s.shortcut)}</button></div>
          ${ui.shortcutError ? `<div class="status-line err">${esc(ui.shortcutError)}</div>` : ""}
          <div class="setting"><div class="t">${t("s.autostart")}<small>${t("s.autostart.hint")}</small></div>${toggle(S.autostart, "toggleAutostart", t("s.autostart"))}</div>
          <div class="setting"><div class="t">${t("s.language")}</div><div class="seg">${lang("fr", "Français")}${lang("en", "English")}</div></div>
        </div>
        <div class="group">
          <div class="setting"><div class="t">${t("s.updates")}<small>${t("s.updates.hint")}</small></div>${toggle(s.checkUpdates, "toggleCheck", t("s.updates"))}</div>
          <div class="setting"><div class="t">${ui.checkMsg ? `<span class="${ui.checkErr ? "" : ""}" style="${ui.checkErr ? "color:var(--danger)" : ""}">${esc(ui.checkMsg)}</span>` : ""}</div>
            <button class="btn" data-act="checkNow">${I.refresh}${t("s.updates.now")}</button></div>
        </div>
        <div class="group">
          <div class="setting"><div class="t">${t("s.config")}<small>${S.portable ? t("s.config.portable") : esc(S.configPath)}</small></div>
            <button class="btn" data-act="openConfig">${I.folder}${t("s.config.open")}</button></div>
        </div>
        <div class="about">${window.mascot("guard")}
          <b>Fixatoutix</b>
          <div>${t("about.version", { version: esc(S.version), build: esc(S.build) })}</div>
          <div class="slogan">${t("about.slogan")}</div>
          <p><button data-act="openRepo">${t("about.repo")}</button></p>
        </div>
      </main>`;
  }

  function sheetHtml() {
    const sh = ui.sheet;
    if (!sh) return "";
    const rule = S.rules.find((r) => r.id === sh.id);
    if (!rule) return "";
    if (sh.type === "rename") {
      return `<div class="scrim"><form class="sheet" data-form="rename" data-id="${rule.id}">
        <h3>${t("rename.title")}</h3>
        <label class="field"><span>${t("rename.label")}</span><input name="label" value="${esc(rule.label)}" maxlength="60" autofocus></label>
        <div class="actions"><button type="button" class="btn" data-act="cancelSheet">${t("cancel")}</button><button class="btn primary">${t("save")}</button></div>
      </form></div>`;
    }
    return `<div class="scrim"><form class="sheet" data-form="match" data-id="${rule.id}">
      <h3>${t("match.title")}</h3>
      <label class="field"><span>${t("match.exe")}</span><input name="exe" value="${esc(rule.match.exe)}" class="${sh.error === "exe" ? "bad" : ""}"></label>
      <label class="field"><span>${t("match.class")}</span><input name="className" value="${esc(rule.match.className || "")}"></label>
      <label class="field"><span>${t("match.title.regex")}</span><input name="titleRegex" value="${esc(rule.match.titleRegex || "")}" class="${sh.error === "regex" ? "bad" : ""}">
        ${sh.error === "regex" ? `<small>${t("match.bad.regex")}</small>` : sh.error === "exe" ? `<small>${t("match.bad.exe")}</small>` : ""}</label>
      <div class="actions"><button type="button" class="btn" data-act="cancelSheet">${t("cancel")}</button><button class="btn primary">${t("save")}</button></div>
    </form></div>`;
  }

  function render() {
    if (!S) return;
    const scroll = root.querySelector("main")?.scrollTop || 0;
    root.innerHTML =
      (ui.view === "picker" ? viewPicker() : ui.view === "settings" ? viewSettings() : viewMain()) + sheetHtml();
    const main = root.querySelector("main");
    if (main) main.scrollTop = scroll;
    root.querySelector(".sheet input")?.focus();
    bindDrag();
  }

  // ---- drag and drop reordering (the first matching rule wins) ----
  function bindDrag() {
    const list = document.getElementById("rules");
    if (!list) return;
    let dragged = null;
    list.addEventListener("dragstart", (e) => {
      dragged = e.target.closest(".rule");
      if (!dragged) return;
      dragged.classList.add("dragging");
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", dragged.dataset.id);
    });
    list.addEventListener("dragover", (e) => {
      if (!dragged) return;
      e.preventDefault();
      const over = e.target.closest(".rule");
      if (!over || over === dragged) return;
      const box = over.getBoundingClientRect();
      list.insertBefore(dragged, e.clientY > box.top + box.height / 2 ? over.nextSibling : over);
    });
    list.addEventListener("dragend", () => {
      if (!dragged) return;
      dragged.classList.remove("dragging");
      dragged = null;
      const ids = [...list.querySelectorAll(".rule")].map((li) => li.dataset.id);
      invoke("reorder_rules", { ids });
    });
  }

  const call = (cmd, args) => invoke(cmd, args).catch((e) => console.warn(cmd, e));

  const actions = {
    toggleGuard: () => call("set_paused", { paused: !S.settings.paused }),
    gather: () => call("gather"),
    async openPicker() {
      ui.windows = await invoke("list_windows");
      ui.view = "picker";
      render();
    },
    async pick(d) {
      await call("nail_window", { hwnd: Number(d.hwnd) });
      ui.view = "main";
      await refresh();
    },
    back() { ui.view = "main"; ui.capture = false; render(); },
    settings() { ui.view = "settings"; ui.checkMsg = ""; render(); },
    toggleRule(d) {
      const r = S.rules.find((x) => x.id === d.id);
      return call("update_rule", { id: d.id, patch: { enabled: !r.enabled } });
    },
    setMode: (d) => call("update_rule", { id: d.id, patch: { mode: d.mode } }),
    more(d) { ui.menu = ui.menu === d.id ? null : d.id; render(); },
    rename(d) { ui.menu = null; ui.sheet = { type: "rename", id: d.id }; render(); },
    editMatch(d) { ui.menu = null; ui.sheet = { type: "match", id: d.id }; render(); },
    async reclou(d) { ui.menu = null; render(); await call("reclou_rule", { id: d.id }); },
    async del(d) { ui.menu = null; await call("delete_rule", { id: d.id }); },
    cancelSheet() { ui.sheet = null; render(); },
    async installUpdate() {
      ui.updating = 0;
      ui.updateError = "";
      render();
      try {
        await invoke("install_update"); // the app restarts itself on success
      } catch (err) {
        ui.updating = null;
        ui.updateError = t("update.failed", { err: String(err) });
        render();
      }
    },
    captureShortcut() { ui.capture = !ui.capture; ui.shortcutError = ""; render(); },
    async toggleAutostart() { await call("set_autostart", { enabled: !S.autostart }); await refresh(); },
    toggleCheck: () => call("set_check_updates", { enabled: !S.settings.checkUpdates }),
    setLang: (d) => call("set_language", { language: d.lang }),
    async checkNow() {
      ui.checkMsg = t("s.updates.checking");
      ui.checkErr = false;
      render();
      try {
        const found = await invoke("check_update");
        ui.checkMsg = found ? t("update.available", { build: found.build }) : t("s.updates.uptodate");
      } catch (err) {
        ui.checkMsg = t("s.updates.fail", { err: String(err) });
        ui.checkErr = true;
      }
      render();
    },
    openConfig: () => call("open_config_dir"),
    openRepo: () => call("open_repo"),
  };

  root.addEventListener("click", (e) => {
    const el = e.target.closest("[data-act]");
    if (ui.menu && !(el && el.dataset.act === "more") && !e.target.closest(".menu")) {
      ui.menu = null;
      if (!el) return render();
    }
    if (el && actions[el.dataset.act]) actions[el.dataset.act](el.dataset, e);
  });

  root.addEventListener("submit", async (e) => {
    e.preventDefault();
    const form = e.target;
    const id = form.dataset.id;
    const data = new FormData(form);
    if (form.dataset.form === "rename") {
      await call("update_rule", { id, patch: { label: String(data.get("label") || "") } });
      ui.sheet = null;
      return refresh();
    }
    try {
      await invoke("set_match", {
        id,
        exe: String(data.get("exe") || ""),
        className: String(data.get("className") || ""),
        titleRegex: String(data.get("titleRegex") || ""),
      });
      ui.sheet = null;
      refresh();
    } catch (err) {
      ui.sheet = { ...ui.sheet, error: String(err) };
      render();
    }
  });

  // ---- keyboard: shortcut capture, Esc to go back / close ----
  function comboFrom(e) {
    if (/^(Control|Alt|Shift|Meta|OS)(Left|Right)?$/.test(e.code)) return null;
    const mods = [];
    if (e.ctrlKey) mods.push("Ctrl");
    if (e.altKey) mods.push("Alt");
    if (e.shiftKey) mods.push("Shift");
    if (e.metaKey) mods.push("Super");
    const key = e.code.startsWith("Key") ? e.code.slice(3) : e.code.startsWith("Digit") ? e.code.slice(5) : e.code;
    return mods.length ? [...mods, key].join("+") : null;
  }

  document.addEventListener("keydown", async (e) => {
    if (ui.capture) {
      e.preventDefault();
      if (e.key === "Escape") { ui.capture = false; return render(); }
      const combo = comboFrom(e);
      if (!combo) return;
      ui.capture = false;
      try {
        await invoke("set_shortcut", { shortcut: combo });
        ui.shortcutError = "";
      } catch {
        ui.shortcutError = t("s.shortcut.taken");
      }
      return refresh();
    }
    if (e.key !== "Escape") return;
    if (ui.sheet) { ui.sheet = null; return render(); }
    if (ui.menu) { ui.menu = null; return render(); }
    if (ui.view !== "main") { ui.view = "main"; return render(); }
    invoke("close_panel");
  });

  document.addEventListener("contextmenu", (e) => e.preventDefault());

  listen("state-changed", () => {
    // Do not yank a sheet or a shortcut capture from under the user's fingers.
    if (!ui.sheet && !ui.capture) refresh();
  });
  listen("update-progress", (e) => {
    const [done, total] = e.payload;
    ui.updating = total ? Math.min(100, Math.round((done * 100) / total)) : 0;
    const label = document.getElementById("update-text");
    if (label) label.textContent = t("update.installing", { pct: ui.updating });
  });

  refresh();
})();
