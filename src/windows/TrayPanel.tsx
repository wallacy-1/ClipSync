import { useEffect, useRef, useState } from "react";
import { Search } from "../components/Icons";
import { useT } from "../i18n";
import { ItemText, Toast, TypeTag } from "../components/ui";
import { pausedUntilText } from "../components/pause";
import { api } from "../lib/ipc";
import { deviceName, relTime } from "../lib/format";
import { usePeers, useVisibleItems, useApp } from "../store/app";
import { CopiedFilesOffer } from "./CopiedFilesOffer";

export function TrayPanel() {
  const t = useT();
  const query = useApp((s) => s.query);
  const setQuery = useApp((s) => s.setQuery);
  const items = useVisibleItems().slice(0, 8);
  const devices = useApp((s) => s.devices);
  const peers = usePeers();
  const me = useApp((s) => s.me);
  const sync = useApp((s) => s.sync);
  const now = useApp((s) => s.now);
  const { copy, send, sendTargets } = useApp.getState();
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => { setActive(0); }, [query]);
  useEffect(() => {
    const focus = () => inputRef.current?.focus();
    focus();
    window.addEventListener("focus", focus);
    return () => window.removeEventListener("focus", focus);
  }, []);

  const paste = async (id: string) => {
    await copy(id);
    await api.windowAction("hide_tray");
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") { setQuery(""); void api.windowAction("hide_tray"); }
    else if (e.key === "ArrowDown") { e.preventDefault(); setActive((a) => Math.min(a + 1, items.length - 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setActive((a) => Math.max(a - 1, 0)); }
    else if (e.key === "Enter" && items[active]) { e.preventDefault(); void paste(items[active].id); }
    else if (e.ctrlKey && e.key.toLowerCase() === "s" && items[active] && peers.length > 0) { e.preventDefault(); void send(items[active].id, sendTargets); }
    else if (/^[1-8]$/.test(e.key) && !query && items[Number(e.key) - 1]) { e.preventDefault(); void paste(items[Number(e.key) - 1].id); }
  };

  const connected = peers.filter((d) => d.state === "connected").length;
  const statusText = sync.paused
    ? (sync.pausedUntil ? t("tray.pausedUntil", { until: pausedUntilText(sync.pausedUntil) }) : t("tray.paused"))
    : connected > 0 ? t("tray.synced") : sync.queuedCount > 0 ? t("tray.queued") : t("tray.waiting");

  return (
    <div className="tray" onKeyDown={onKey}>
      <div className="tray-head">
        <div className="brand-dot" />
        <span className="title">ClipSync</span>
        {(peers.length > 0 || sync.paused) && (
          <span className={`status-inline${connected > 0 && !sync.paused ? "" : " warn"}`}>
            <span className={`dot ${connected > 0 && !sync.paused ? "online pulse" : "warn"}`} />
            {statusText}
          </span>
        )}
      </div>
      <label className="tray-search">
        <Search size={15} style={{ color: "var(--color-neutral-600)" }} />
        <input ref={inputRef} value={query} onChange={(e) => setQuery(e.target.value)} placeholder={t("tray.search")} />
      </label>
      <div className="tray-list">
        <CopiedFilesOffer variant="inline" />
        {items.length === 0 && <div className="muted" style={{ padding: "18px 12px", fontSize: 13 }}>{t("tray.empty")}</div>}
        {items.map((it, i) => (
          <button key={it.id} className={`tray-row${i === active ? " active" : ""}`} onMouseEnter={() => setActive(i)} onClick={() => paste(it.id)}>
            <span className="key">{i + 1}</span>
            <div className="body">
              <ItemText item={it} />
              <div className="meta">
                {it.isSensitive ? t("item.hidden") : deviceName(devices, it.sourceDeviceId, me?.deviceId ?? "")} · {relTime(it.createdAt, now)}
              </div>
            </div>
            <TypeTag item={it} />
          </button>
        ))}
      </div>
      <div className="tray-foot">
        <span><b>↑↓</b> {t("tray.navigate")}</span>
        <span><b>Enter</b> {t("tray.paste")}</span>
        {peers.length > 0 && <span><b>Ctrl+S</b> {t("tray.send")}</span>}
        <button className="open" onClick={() => api.windowAction("show_main")}>{t("tray.openWindow")}</button>
      </div>
      <Toast />
    </div>
  );
}
