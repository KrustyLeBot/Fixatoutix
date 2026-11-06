// Reads ?k=ok|error|info&t=title&s=subtitle (set by the Rust side) and draws the toast.
(function () {
  const q = new URLSearchParams(location.search);
  const kind = q.get("k") || "ok";
  const box = document.getElementById("toast");
  box.className = "toast " + kind;
  // ok: proud blacksmith, error: annoyed blacksmith, info: watchful blacksmith.
  const mood = kind === "error" ? "angry" : "guard";
  box.innerHTML = window.mascot(mood);
  const text = document.createElement("div");
  text.className = "t";
  const title = document.createElement("b");
  title.textContent = q.get("t") || "";
  const sub = document.createElement("span");
  sub.textContent = q.get("s") || "";
  text.append(title, sub);
  box.append(text);
})();
