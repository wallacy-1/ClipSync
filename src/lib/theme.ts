import { api, inTauri } from "./ipc";
import type { ThemePref } from "./types";

const KEY = "clipsync-theme";
const query = () => window.matchMedia("(prefers-color-scheme: dark)");

/** Tema resolvido que o `index.html` lê antes do primeiro paint. */
function remember(resolved: "light" | "dark") {
  try { localStorage.setItem(KEY, resolved); } catch { /* sem armazenamento: só perde o anti-flash */ }
}

/** Aplica a preferência e, em "system", segue o sistema; devolve a função que desfaz o listener. */
export function applyTheme(pref: ThemePref): () => void {
  const set = (dark: boolean) => {
    const resolved = dark ? "dark" : "light";
    document.documentElement.dataset.theme = resolved;
    remember(resolved);
    // Ícones das barras do sistema (Android) seguem o tema do app, não o do sistema.
    if (inTauri) void api.setSystemBars(dark).catch(() => {});
  };
  if (pref !== "system") {
    set(pref === "dark");
    return () => {};
  }
  const mq = query();
  set(mq.matches);
  const onChange = (e: MediaQueryListEvent) => set(e.matches);
  mq.addEventListener("change", onChange);
  return () => mq.removeEventListener("change", onChange);
}
