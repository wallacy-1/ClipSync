import { useEffect, useState } from "react";
import { useT } from "./i18n";
import { applyTheme } from "./lib/theme";
import { AndroidShell } from "./android/AndroidShell";
import { errorText } from "./lib/ipc";
import { useApp } from "./store/app";
import { TrayPanel } from "./windows/TrayPanel";
import { WinShell } from "./windows/WinShell";

const isTrayWindow = location.hash.startsWith("#/tray");
// Sem resposta do `bootstrap` até aqui, a inicialização é tida como travada.
const BOOT_TIMEOUT_MS = 8000;

type NativeBridge = { restart(): void };

/** Android: a ponte Kotlin mata e reabre o processo (destrava até o núcleo Rust).
 *  Sem ela (desktop, ou WebView criada antes da ponte), recarrega a página. */
function restartApp() {
  const native = (window as { ClipSyncNative?: NativeBridge }).ClipSyncNative;
  if (native) native.restart();
  else location.reload();
}

export default function App() {
  const ready = useApp((s) => s.ready);
  const platform = useApp((s) => s.platform);
  const init = useApp((s) => s.init);
  const theme = useApp((s) => s.settings?.theme);
  const t = useT();
  const [error, setError] = useState<string | null>(null);
  const [stuck, setStuck] = useState(false);
  const [restarting, setRestarting] = useState(false);

  // Antes do bootstrap vale o tema que o index.html leu do localStorage.
  useEffect(() => (theme ? applyTheme(theme) : undefined), [theme]);

  useEffect(() => {
    init().catch((e) => setError(errorText(e)));
  }, [init]);

  useEffect(() => {
    if (ready) return;
    const id = setTimeout(() => setStuck(true), BOOT_TIMEOUT_MS);
    return () => clearTimeout(id);
  }, [ready]);

  // Resposta atrasada ainda vale: `ready` ganha do erro e do tempo limite.
  if (!ready && (error !== null || stuck)) {
    return (
      <div className="loading boot-failed">
        <div className="brand-dot" />
        <b>{t(error !== null ? "app.startFailed" : "app.startStuck")}</b>
        {error && <span className="boot-error">{error}</span>}
        <button
          className="btn btn-primary"
          disabled={restarting}
          onClick={() => {
            setRestarting(true);
            restartApp();
          }}
        >
          {t(restarting ? "app.restarting" : "app.retry")}
        </button>
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
