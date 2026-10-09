import { Copy, External, Mail, Pin, Send, Trash } from "../components/Icons";
import { ItemImage, TypeTag, useMasked } from "../components/ui";
import { useT } from "../i18n";
import { countdown, deviceName, imageLabel, isMono, isOpenable, relTime } from "../lib/format";
import { usePeers, useApp } from "../store/app";

export function ItemSheet({ itemId, onClose, onSend }: { itemId: string; onClose: () => void; onSend: (id: string) => void }) {
  const t = useT();
  const item = useApp((s) => s.items.find((i) => i.id === itemId) ?? null);
  const devices = useApp((s) => s.devices);
  const me = useApp((s) => s.me);
  const now = useApp((s) => s.now);
  const hideSensitive = useApp((s) => s.settings?.hideSensitive ?? true);
  const hasPeers = usePeers().length > 0;
  const { copy, pin, remove, reveal, hide, openItem } = useApp.getState();
  const { masked, text } = useMasked(item);
  if (!item) return null;

  return (
    <div className="sheet-backdrop" onClick={() => { if (item.isSensitive) hide(item.id); onClose(); }}>
      <div className="sheet" onClick={(e) => e.stopPropagation()}>
        <div className="handle" />
        <div className="row-flex" style={{ gap: 9, marginBottom: 8 }}>
          {item.isSensitive ? <span className="tag tag-sensitive">{t("item.sensitive")}</span> : <TypeTag item={item} />}
          <span className="muted" style={{ fontSize: 12.5 }}>
            {deviceName(devices, item.sourceDeviceId, me?.deviceId ?? "")} · {relTime(item.createdAt, now)}
            {item.expiresAt ? ` · ${t("item.expiresIn", { time: countdown(item.expiresAt, now) })}` : ""}
          </span>
        </div>
        {item.contentType === "image" ? (
          <div className="preview img">
            <ItemImage item={item} full />
            <span>{imageLabel(item)}</span>
          </div>
        ) : (
          <div className={`preview${isMono(item.contentType) ? "" : " body"}`} style={{ maxHeight: 200, overflow: "auto", userSelect: "text", whiteSpace: "pre-wrap" }}>{text}</div>
        )}
        {item.isSensitive && hideSensitive && (
          <button className="btn btn-secondary btn-block" style={{ marginBottom: 10 }} onClick={() => (masked ? reveal(item.id) : hide(item.id))}>
            {masked ? t("android.item.showContent") : t("history.hide")}
          </button>
        )}
        <div className="actions-grid">
          <button className="btn btn-primary" onClick={async () => { await copy(item.id); onClose(); }}><Copy size={17} /> {t("history.copy")}</button>
          {isOpenable(item) && (
            <button className="btn btn-secondary" disabled={masked} onClick={() => openItem(item.id)}>
              {item.contentType === "email" ? <><Mail size={17} /> {t("history.writeEmail")}</> : <><External size={17} /> {t("common.open")}</>}
            </button>
          )}
          {hasPeers && <button className="btn btn-secondary" disabled={masked} onClick={() => onSend(item.id)}><Send size={17} /> {t("common.send")}</button>}
          <button className="btn btn-secondary" onClick={() => pin(item.id, !item.isPinned)}><Pin size={17} /> {t(item.isPinned ? "history.unpin" : "history.pin")}</button>
          <button className="btn btn-secondary" style={{ color: "var(--color-danger-fg)" }} onClick={async () => { await remove(item.id); onClose(); }}><Trash size={17} /> {t("history.delete")}</button>
        </div>
      </div>
    </div>
  );
}
