import { File } from "../components/Icons";
import { bytes, pluralize } from "../lib/format";
import { useApp } from "../store/app";
import { useFileTargets } from "./SendFileDialog";

/** A oferta some sozinha depois disso: o Ctrl+C provavelmente já virou um Ctrl+V em outra pasta. */
const OFFER_TTL_MS = 10 * 60 * 1000;
const MAX_TARGETS = 3;

/**
 * Arquivos copiados no Explorer: oferece "Enviar para…" (nunca envia sozinho).
 * `float` = cartão no canto da janela principal; `inline` = linha no topo do painel rápido.
 */
export function CopiedFilesOffer({ variant }: { variant: "float" | "inline" }) {
  const offer = useApp((s) => s.copiedFiles);
  const now = useApp((s) => s.now);
  const send = useApp((s) => s.sendCopiedFiles);
  const dismiss = useApp((s) => s.dismissCopiedFiles);
  const { targets } = useFileTargets();
  if (!offer || offer.files.length === 0 || now - offer.copiedAt > OFFER_TTL_MS) return null;

  // Conectados primeiro; offline também serve (entra na fila e sai ao reconectar).
  const options = [...targets.filter((t) => t.selectable), ...targets.filter((t) => !t.selectable && t.cap === "Offline")].slice(0, MAX_TARGETS);
  if (options.length === 0) return null;

  const total = offer.files.reduce((n, f) => n + f.size, 0);
  const title = offer.files.length === 1 ? offer.files[0].name : `${pluralize(offer.files.length, "arquivo copiado", "arquivos copiados")}`;
  const notes = [
    offer.skippedDirs > 0 ? `${pluralize(offer.skippedDirs, "pasta fica", "pastas ficam")} de fora` : null,
    offer.skippedFiles > 0 ? `+${offer.skippedFiles} além do limite` : null,
  ].filter(Boolean).join(" · ");

  return (
    <div className={`copied-offer ${variant}`} role="status">
      <div className="co-head">
        <div className="co-icon"><File size={16} /></div>
        <div className="co-body">
          <div className="co-title" title={offer.files.map((f) => f.name).join("\n")}>{title}</div>
          <div className="co-meta">{bytes(total)}{notes ? ` · ${notes}` : ""}</div>
        </div>
        <button className="co-close" title="Dispensar" onClick={dismiss}>
          <svg width="12" height="12" viewBox="0 0 12 12" stroke="currentColor" strokeWidth="1.4"><path d="M2 2l8 8M10 2l-8 8" /></svg>
        </button>
      </div>
      <div className="co-actions">
        {options.map((t) => (
          <button key={t.device.deviceId} className="btn btn-secondary co-send" onClick={() => void send(t.device.deviceId)} title={t.cap === "Offline" ? "Offline · entra na fila" : t.detail}>
            Enviar para {t.device.name}{t.cap === "Offline" ? " (fila)" : ""}
          </button>
        ))}
      </div>
    </div>
  );
}
