import { Copy, Pin, Send, Trash } from "../components/Icons";
import { useMasked } from "../components/ui";
import { TYPE_LABEL, countdown, deviceName, isMono, relTime } from "../lib/format";
import { useApp } from "../store/app";

export function ItemSheet({ itemId, onClose, onSend }: { itemId: string; onClose: () => void; onSend: (id: string) => void }) {
  const item = useApp((s) => s.items.find((i) => i.id === itemId) ?? null);
  const devices = useApp((s) => s.devices);
  const me = useApp((s) => s.me);
  const now = useApp((s) => s.now);
  const hideSensitive = useApp((s) => s.settings?.hideSensitive ?? true);
  const { copy, pin, remove, reveal, hide } = useApp.getState();
  const { masked, text } = useMasked(item);
  if (!item) return null;

  return (
    <div className="sheet-backdrop" onClick={() => { if (item.isSensitive) hide(item.id); onClose(); }}>
      <div className="sheet" onClick={(e) => e.stopPropagation()}>
        <div className="handle" />
        <div className="row-flex" style={{ gap: 9, marginBottom: 8 }}>
          {item.isSensitive ? <span className="tag tag-sensitive">Sensível</span> : <span className="tag">{TYPE_LABEL[item.contentType]}</span>}
          <span className="muted" style={{ fontSize: 12.5 }}>
            {deviceName(devices, item.sourceDeviceId, me?.deviceId ?? "")} · {relTime(item.createdAt, now)}
            {item.expiresAt ? ` · expira em ${countdown(item.expiresAt, now)}` : ""}
          </span>
        </div>
        <div className={`preview${isMono(item.contentType) ? "" : " body"}`} style={{ maxHeight: 200, overflow: "auto", userSelect: "text", whiteSpace: "pre-wrap" }}>{text}</div>
        {item.isSensitive && hideSensitive && (
          <button className="btn btn-secondary btn-block" style={{ marginBottom: 10 }} onClick={() => (masked ? reveal(item.id) : hide(item.id))}>
            {masked ? "Mostrar conteúdo" : "Ocultar"}
          </button>
        )}
        <div className="actions-grid">
          <button className="btn btn-primary" onClick={async () => { await copy(item.id); onClose(); }}><Copy size={17} /> Copiar</button>
          <button className="btn btn-secondary" disabled={masked} onClick={() => onSend(item.id)}><Send size={17} /> Enviar</button>
          <button className="btn btn-secondary" onClick={() => pin(item.id, !item.isPinned)}><Pin size={17} /> {item.isPinned ? "Desafixar" : "Fixar"}</button>
          <button className="btn btn-secondary" style={{ color: "var(--color-danger-fg)" }} onClick={async () => { await remove(item.id); onClose(); }}><Trash size={17} /> Apagar</button>
        </div>
      </div>
    </div>
  );
}
