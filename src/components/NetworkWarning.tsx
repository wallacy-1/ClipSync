import { useEffect, useState } from "react";
import { useT } from "../i18n";
import { WifiOff } from "./Icons";

/** Aviso quando o aparelho perde a rede; volta a aparecer a cada nova queda. */
export function NetworkWarning() {
  const t = useT();
  const [offline, setOffline] = useState(() => !navigator.onLine);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    const down = () => { setOffline(true); setDismissed(false); };
    const up = () => setOffline(false);
    window.addEventListener("offline", down);
    window.addEventListener("online", up);
    return () => {
      window.removeEventListener("offline", down);
      window.removeEventListener("online", up);
    };
  }, []);

  if (!offline || dismissed) return null;
  return (
    <div className="dialog-backdrop" onClick={() => setDismissed(true)}>
      <div className="dialog" onClick={(e) => e.stopPropagation()}>
        <div className="dialog-title row-flex" style={{ gap: 10 }}>
          <WifiOff size={22} style={{ color: "var(--color-accent-800)" }} /> {t("net.title")}
        </div>
        <div className="dialog-body">{t("net.body")}</div>
        <div className="dialog-actions">
          <button className="btn btn-primary" onClick={() => setDismissed(true)}>{t("net.gotIt")}</button>
        </div>
      </div>
    </div>
  );
}
