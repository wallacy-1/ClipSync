import { File } from "./Icons";
import { bytes, deviceName } from "../lib/format";
import { useApp } from "../store/app";

/** Arquivo recebido esperando "Aceitar/Recusar". Mostra o mais antigo; os demais esperam. */
export function AskPrompt({ variant }: { variant: "float" | "android" }) {
  const asks = useApp((s) => s.asks);
  const transfers = useApp((s) => s.transfers);
  const devices = useApp((s) => s.devices);
  const selfId = useApp((s) => s.me?.deviceId ?? "");
  const accept = useApp((s) => s.acceptTransfer);
  const decline = useApp((s) => s.declineTransfer);

  const pending = asks.map((id) => transfers.find((t) => t.id === id)).filter((t) => t && t.state.kind === "incoming");
  const t = pending[0];
  if (!t) return null;
  const from = deviceName(devices, t.deviceId, selfId);

  return (
    <div className={`ask-card ${variant}`} role="alertdialog" aria-label={`${from} quer enviar ${t.safeName}`}>
      <div className="co-head">
        <div className="co-icon"><File size={16} /></div>
        <div className="co-body">
          <div className="ask-from">{from} quer enviar{pending.length > 1 ? ` · +${pending.length - 1}` : ""}</div>
          <div className="co-title" title={t.originalName}>{t.safeName}</div>
          <div className="co-meta">{bytes(t.size)}</div>
        </div>
      </div>
      <div className="ask-actions">
        <button className="btn btn-secondary" onClick={() => void decline(t.id)}>Recusar</button>
        <button className="btn btn-primary" onClick={() => void accept(t.id)}>Aceitar</button>
      </div>
    </div>
  );
}
