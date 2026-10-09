import { File } from "./Icons";
import { useT } from "../i18n";
import { bytes, deviceName } from "../lib/format";
import { useApp } from "../store/app";

/** Arquivo recebido esperando "Aceitar/Recusar". Mostra o mais antigo; os demais esperam. */
export function AskPrompt({ variant }: { variant: "float" | "android" }) {
  const t = useT();
  const asks = useApp((s) => s.asks);
  const transfers = useApp((s) => s.transfers);
  const devices = useApp((s) => s.devices);
  const selfId = useApp((s) => s.me?.deviceId ?? "");
  const accept = useApp((s) => s.acceptTransfer);
  const decline = useApp((s) => s.declineTransfer);

  const pending = asks.map((id) => transfers.find((x) => x.id === id)).filter((x) => x && x.state.kind === "incoming");
  const rec = pending[0];
  if (!rec) return null;
  const from = deviceName(devices, rec.deviceId, selfId);

  return (
    <div className={`ask-card ${variant}`} role="alertdialog" aria-label={t("ask.wantsToSendFile", { from, file: rec.safeName })}>
      <div className="co-head">
        <div className="co-icon"><File size={16} /></div>
        <div className="co-body">
          <div className="ask-from">{t("ask.wantsToSend", { from })}{pending.length > 1 ? ` · +${pending.length - 1}` : ""}</div>
          <div className="co-title" title={rec.originalName}>{rec.safeName}</div>
          <div className="co-meta">{bytes(rec.size)}</div>
        </div>
      </div>
      <div className="ask-actions">
        <button className="btn btn-secondary" onClick={() => void decline(rec.id)}>{t("common.decline")}</button>
        <button className="btn btn-primary" onClick={() => void accept(rec.id)}>{t("common.accept")}</button>
      </div>
    </div>
  );
}
