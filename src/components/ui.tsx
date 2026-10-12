import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { useT } from "../i18n";
import { api, errorText } from "../lib/ipc";
import { MASK, STATE_LABEL, TYPE_LABEL, imageLabel, isMono } from "../lib/format";
import type { ClipboardItem, Device, SyncState } from "../lib/types";
import { useApp } from "../store/app";
import { Check, Monitor, Phone } from "./Icons";

/** `**negrito**` dentro de um texto traduzido vira <b>. */
export function rich(text: string): ReactNode {
  return text.split("**").map((part, i) => (i % 2 === 1 ? <b key={i}>{part}</b> : part));
}

export function TypeTag({ item }: { item: ClipboardItem }) {
  const t = useT();
  const formatted = item.metadata.formatted === true && !item.isSensitive;
  return (
    <>
      <span className="tag">{TYPE_LABEL[item.contentType]}</span>
      {formatted && <span className="tag" title={t("item.formattedHint")}>{t("item.formatted")}</span>}
    </>
  );
}

const STATE_CLASS: Record<SyncState, string> = {
  synced: "tag-synced",
  sending: "tag-sending",
  queued: "tag-queued",
  local: "",
};

export function StateTag({ item }: { item: ClipboardItem }) {
  const t = useT();
  // Imagem não vai em `Clip`: não há Ack para acompanhar, o estado de sync não se aplica.
  if (item.contentType === "image") return null;
  if (item.isSensitive) return <span className="tag tag-sensitive">{t("item.sensitive")}</span>;
  return <span className={`tag ${STATE_CLASS[item.syncState]}`}>{STATE_LABEL[item.syncState]}</span>;
}

export function Toggle({ on, onChange, disabled }: { on: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      className={`toggle${on ? " on" : ""}`}
      disabled={disabled}
      onClick={() => onChange(!on)}
    >
      <span className="knob" />
    </button>
  );
}

/** Linha "rótulo + dica + toggle" das opções por dispositivo. `globalOff`: o ajuste geral desligado vence. */
export function DeviceFlag({ label, help, on, globalOff, disabled, onChange }: {
  label: string; help: string; on: boolean; globalOff: boolean; disabled?: boolean; onChange: (v: boolean) => void;
}) {
  const t = useT();
  return (
    <div className="dev-flag">
      <div className="txt">
        <span className="lbl">{label}</span>
        <small>{globalOff ? t("device.globalOff") : help}</small>
      </div>
      <Toggle on={on} disabled={disabled} onChange={onChange} />
    </div>
  );
}

/** `passive`: só o desenho, para dentro de um botão que já alterna (botão dentro de botão é HTML inválido). */
export function CheckBox({ on, onChange, small, disabled, passive }: { on: boolean; onChange: (v: boolean) => void; small?: boolean; disabled?: boolean; passive?: boolean }) {
  if (passive) {
    return (
      <span role="checkbox" aria-checked={on} className={`check${on ? " on" : ""}${small ? " sm" : ""}`}>
        {on && <Check size={small ? 12 : 15} strokeWidth={3.5} />}
      </span>
    );
  }
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={on}
      className={`check${on ? " on" : ""}${small ? " sm" : ""}`}
      disabled={disabled}
      onClick={() => onChange(!on)}
    >
      {on && <Check size={small ? 12 : 15} strokeWidth={3.5} />}
    </button>
  );
}

export function Dot({ device }: { device: Device }) {
  const cls = device.state === "connected" ? "online" : device.state === "connecting" ? "warn" : "";
  return <span className={`dot ${cls}`} />;
}

export function Avatar({ device }: { device: Device }) {
  const online = device.state === "connected";
  const Icon = device.platform === "android" || device.platform === "ios" ? Phone : Monitor;
  return (
    <div className={`avatar${online ? " online" : ""}`}>
      <Icon size={20} />
    </div>
  );
}

export function DeviceBadge({ device }: { device: Device }) {
  const t = useT();
  if (device.isSelf) return <span className="tag tag-synced">{t("device.active")}</span>;
  if (device.revoked && device.state !== "connected") return <span className="tag tag-failed">{t("device.revoked")}</span>;
  switch (device.state) {
    case "connected": return <span className="tag tag-synced">{t("device.connected")}</span>;
    case "connecting": return <span className="tag tag-sending">{t("device.connecting")}</span>;
    case "pairing": return <span className="tag tag-sending">{t("device.pairing")}</span>;
    case "blocked": return <span className="tag tag-failed">{t("device.blocked")}</span>;
    case "discovered": return <span className="tag">{t("device.onNetwork")}</span>;
    default:
      return device.lastError ? <span className="tag tag-failed">{t("device.failed")}</span> : <span className="tag tag-queued">{t("device.offline")}</span>;
  }
}

/** Miniatura de um item `image` (cache no store); com `full`, troca pela imagem inteira quando ela chega. */
export function ItemImage({ item, full, className = "" }: { item: ClipboardItem; full?: boolean; className?: string }) {
  const thumb = useApp((s) => s.thumbs[item.id]);
  const loadThumb = useApp((s) => s.loadThumb);
  useEffect(() => { if (!thumb) void loadThumb(item.id); }, [item.id, thumb, loadThumb]);
  const big = useAsync(() => (full ? api.itemImage(item.id, true) : Promise.resolve(null)), [item.id, full]);
  const src = big.data ?? thumb;
  return src
    ? <img className={`thumb${full ? " full" : ""} ${className}`} src={src} alt="" draggable={false} />
    : <div className={`thumb ph ${className}`} />;
}

/** Texto do item respeitando ocultação de conteúdo sensível. */
export function ItemText({ item, className = "" }: { item: ClipboardItem; className?: string }) {
  const revealed = useApp((s) => s.revealed[item.id]);
  const hide = useApp((s) => s.settings?.hideSensitive ?? true);
  const masked = item.isSensitive && hide && !revealed;
  if (item.contentType === "image") {
    return <div className={`item-img ${className}`}><ItemImage item={item} /><span className="item-text">{imageLabel(item)}</span></div>;
  }
  const text = masked ? MASK : (revealed ?? item.content).split(/\r?\n/).find((l) => l.trim()) ?? "";
  return <div className={`item-text${isMono(item.contentType) ? " mono" : ""}${masked ? " dim" : ""} ${className}`}>{text}</div>;
}

export function useMasked(item: ClipboardItem | null | undefined) {
  const revealed = useApp((s) => (item ? s.revealed[item.id] : undefined));
  const hide = useApp((s) => s.settings?.hideSensitive ?? true);
  if (!item) return { masked: false, text: "" };
  const masked = item.isSensitive && hide && !revealed;
  return { masked, text: masked ? MASK : revealed ?? item.content };
}

export function Toast() {
  const toast = useApp((s) => s.toast);
  if (!toast) return null;
  return <div className={`toast${toast.kind === "error" ? " error" : ""}`}>{toast.text}</div>;
}

export function Confirm({
  title, body, confirmLabel, danger, onConfirm, onCancel,
}: { title: string; body: ReactNode; confirmLabel?: string; danger?: boolean; onConfirm: () => void; onCancel: () => void }) {
  const t = useT();
  return (
    <div className="dialog-backdrop" onClick={onCancel}>
      <div className="dialog" onClick={(e) => e.stopPropagation()}>
        <div className="dialog-title">{title}</div>
        <div className="dialog-body">{body}</div>
        <div className="dialog-actions">
          <button className="btn btn-secondary" onClick={onCancel}>{t("common.cancel")}</button>
          <button className="btn btn-primary" style={danger ? { background: "var(--color-danger-fg)" } : undefined} onClick={onConfirm}>
            {confirmLabel ?? t("common.confirm")}
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * Diálogo com um campo de texto. `onSubmit` rejeita para recusar o valor: o erro aparece no
 * diálogo, que continua aberto; quem chama fecha em caso de sucesso.
 */
export function Prompt({
  title, body, initial = "", placeholder, inputMode, confirmLabel, onSubmit, onCancel,
}: {
  title: string; body?: ReactNode; initial?: string; placeholder?: string; inputMode?: "url" | "text";
  confirmLabel?: string; onSubmit: (value: string) => Promise<void>; onCancel: () => void;
}) {
  const t = useT();
  const [value, setValue] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await onSubmit(value);
    } catch (e) {
      setError(errorText(e));
      setBusy(false);
    }
  };
  return (
    <div className="dialog-backdrop" onClick={onCancel}>
      <form className="dialog" onClick={(e) => e.stopPropagation()} onSubmit={(e) => { e.preventDefault(); void submit(); }}>
        <div className="dialog-title">{title}</div>
        {body && <div className="dialog-body">{body}</div>}
        <input className="input" autoFocus value={value} placeholder={placeholder} inputMode={inputMode} onChange={(e) => setValue(e.target.value)} />
        {error && <div className="dialog-error">{error}</div>}
        <div className="dialog-actions">
          <button type="button" className="btn btn-secondary" onClick={onCancel}>{t("common.cancel")}</button>
          <button type="submit" className="btn btn-primary" disabled={busy}>{confirmLabel ?? t("common.save")}</button>
        </div>
      </form>
    </div>
  );
}

export function useAsync<T>(fn: () => Promise<T>, deps: unknown[]): { data: T | null; error: string | null; loading: boolean } {
  const [state, setState] = useState<{ data: T | null; error: string | null; loading: boolean }>({ data: null, error: null, loading: true });
  useEffect(() => {
    let alive = true;
    setState((s) => ({ ...s, loading: true }));
    fn().then(
      (data) => alive && setState({ data, error: null, loading: false }),
      (e) => alive && setState({ data: null, error: String(e), loading: false }),
    );
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return state;
}
