// Reads ?k=ok|error|info&t=title&s=subtitle (set by the Rust side) and draws the toast.
(function () {
  const q = new URLSearchParams(location.search);
  const kind = q.get("k") || "ok";
  const box = document.getElementById("toast");
  box.className = "toast " + kind;
  const glyph = {
    ok: '<path d="M4 10.5l4 4 8-9"/>',
    error: '<circle cx="10" cy="10" r="7.5"/><path d="M10 6v5M10 13.6v.1"/>',
    info: '<path d="M10 3v9M6 8l4 4 4-4M4 16h12"/>',
  }[kind] || "";
  box.innerHTML = `<span class="ico"><svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${glyph}</svg></span>`;
  const text = document.createElement("div");
  text.className = "t";
  const title = document.createElement("b");
  title.textContent = q.get("t") || "";
  const sub = document.createElement("span");
  sub.textContent = q.get("s") || "";
  text.append(title, sub);
  box.append(text);
})();
