import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { MASK, STATE_LABEL, TYPE_LABEL, isMono } from "../lib/format";
import type { ClipboardItem, Device, SyncState } from "../lib/types";
import { useApp } from "../store/app";
import { Check, Monitor, Phone } from "./Icons";

export function TypeTag({ item }: { item: ClipboardItem }) {
  return <span className="tag">{TYPE_LABEL[item.contentType]}</span>;
}

const STATE_CLASS: Record<SyncState, string> = {
  synced: "tag-synced",
  sending: "tag-sending",
  queued: "tag-queued",
  failed: "tag-failed",
  local: "",
};

export function StateTag({ item }: { item: ClipboardItem }) {
  if (item.isSensitive) return <span className="tag tag-sensitive">Sensível</span>;
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

export function CheckBox({ on, onChange, small, disabled }: { on: boolean; onChange: (v: boolean) => void; small?: boolean; disabled?: boolean }) {
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
  if (device.isSelf) return <span className="tag tag-synced">Ativo</span>;
  switch (device.state) {
    case "connected": return <span className="tag tag-synced">Conectado</span>;
    case "connecting": return <span className="tag tag-sending">Conectando</span>;
    case "pairing": return <span className="tag tag-sending">Pareando</span>;
    case "blocked": return <span className="tag tag-failed">Bloqueado</span>;
    case "discovered": return <span className="tag">Na rede</span>;
    default:
      return device.lastError ? <span className="tag tag-failed">Falhou</span> : <span className="tag tag-queued">Offline</span>;
  }
}

/** Texto do item respeitando ocultação de conteúdo sensível. */
export function ItemText({ item, className = "" }: { item: ClipboardItem; className?: string }) {
  const revealed = useApp((s) => s.revealed[item.id]);
  const hide = useApp((s) => s.settings?.hideSensitive ?? true);
  const masked = item.isSensitive && hide && !revealed;
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
  title, body, confirmLabel = "Confirmar", danger, onConfirm, onCancel,
}: { title: string; body: ReactNode; confirmLabel?: string; danger?: boolean; onConfirm: () => void; onCancel: () => void }) {
  return (
    <div className="dialog-backdrop" onClick={onCancel}>
      <div className="dialog" onClick={(e) => e.stopPropagation()}>
        <div className="dialog-title">{title}</div>
        <div className="dialog-body">{body}</div>
        <div className="dialog-actions">
          <button className="btn btn-secondary" onClick={onCancel}>Cancelar</button>
          <button className="btn btn-primary" style={danger ? { background: "var(--color-danger-fg)" } : undefined} onClick={onConfirm}>
            {confirmLabel}
          </button>
        </div>
      </div>
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
