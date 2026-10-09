import { useEffect, useState } from "react";
import { useT } from "./i18n";
import { applyTheme } from "./lib/theme";
import { AndroidShell } from "./android/AndroidShell";
import { errorText } from "./lib/ipc";
import { useApp } from "./store/app";
import { TrayPanel } from "./windows/TrayPanel";
import { WinShell } from "./windows/WinShell";

const isTrayWindow = location.hash.startsWith("#/tray");

export default function App() {
  const ready = useApp((s) => s.ready);
  const platform = useApp((s) => s.platform);
  const init = useApp((s) => s.init);
  const theme = useApp((s) => s.settings?.theme);
  const t = useT();
  const [error, setError] = useState<string | null>(null);

  // Antes do bootstrap vale o tema que o index.html leu do localStorage.
  useEffect(() => (theme ? applyTheme(theme) : undefined), [theme]);

  useEffect(() => {
    init().catch((e) => setError(errorText(e)));
  }, [init]);

  if (error) {
    return (
      <div className="loading" style={{ flexDirection: "column", padding: 32, textAlign: "center" }}>
        <div className="brand-dot" />
        <b>{t("app.startFailed")}</b>
        <span>{error}</span>
      </div>
    );
  }
  if (!ready) {
    return (
      <div className="loading">
        <div className="brand-dot pulse" /> ClipSync
      </div>
    );
  }
  // Sem `key` por idioma: remontar zeraria o estado local. `useT()` assina `lang` e nada usa `memo`,
  // então a árvore re-renderiza no lugar.
  if (isTrayWindow) return <TrayPanel />;
  return platform === "android" || platform === "ios" ? <AndroidShell /> : <WinShell />;
}
