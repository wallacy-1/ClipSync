import { useEffect, useState } from "react";
import { WifiOff } from "./Icons";

/** Aviso quando o aparelho perde a rede; volta a aparecer a cada nova queda. */
export function NetworkWarning() {
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
          <WifiOff size={22} style={{ color: "var(--color-accent-800)" }} /> Sem conexão
        </div>
        <div className="dialog-body">
          Este dispositivo não está conectado a nenhuma rede. A sincronização só funciona com os aparelhos na mesma Wi-Fi; o que você copiar fica na fila até a rede voltar.
        </div>
        <div className="dialog-actions">
          <button className="btn btn-primary" onClick={() => setDismissed(true)}>Entendi</button>
        </div>
      </div>
    </div>
  );
}
