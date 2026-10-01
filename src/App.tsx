import { useEffect, useState } from "react";
import { AndroidShell } from "./android/AndroidShell";
import { useApp } from "./store/app";
import { TrayPanel } from "./windows/TrayPanel";
import { WinShell } from "./windows/WinShell";

const isTrayWindow = location.hash.startsWith("#/tray");

export default function App() {
  const ready = useApp((s) => s.ready);
  const platform = useApp((s) => s.platform);
  const init = useApp((s) => s.init);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    init().catch((e) => setError(String(e)));
  }, [init]);

  if (error) {
    return (
      <div className="loading" style={{ flexDirection: "column", padding: 32, textAlign: "center" }}>
        <div className="brand-dot" />
        <b>O ClipSync não conseguiu iniciar.</b>
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
  if (isTrayWindow) return <TrayPanel />;
  return platform === "android" || platform === "ios" ? <AndroidShell /> : <WinShell />;
}
