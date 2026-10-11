import { useEffect } from "react";
import { useT, type Key } from "../i18n";
import { api, startDragging } from "../lib/ipc";
import { shortTime } from "../lib/format";
import { usePeers, useApp, type View } from "../store/app";
import { Clipboard, Monitor, Pin, Sliders, Transfers } from "../components/Icons";
import { NetworkWarning } from "../components/NetworkWarning";
import { PairPrompts } from "../components/PairPrompts";
import { Dot, Toast } from "../components/ui";
import { HistoryView } from "./HistoryView";
import { DevicesView } from "./DevicesView";
import { TransfersView } from "./TransfersView";
import { SettingsView } from "./SettingsView";
import { CopiedFilesOffer } from "./CopiedFilesOffer";
import { AskPrompt } from "../components/AskPrompt";

const NAV: { id: View; label: Key; hint?: string; Icon: typeof Clipboard }[] = [
  { id: "history", label: "nav.history", hint: "Ctrl+1", Icon: Clipboard },
  { id: "pinned", label: "nav.pinned", Icon: Pin },
  { id: "devices", label: "nav.devices", hint: "Ctrl+2", Icon: Monitor },
  { id: "transfers", label: "nav.transfers", hint: "Ctrl+3", Icon: Transfers },
  { id: "settings", label: "nav.settings", hint: "Ctrl+4", Icon: Sliders },
];

export function WinShell() {
  const t = useT();
  const view = useApp((s) => s.view);
  const setView = useApp((s) => s.setView);
  const me = useApp((s) => s.me);
  const peers = usePeers();
  const sync = useApp((s) => s.sync);
  const pinned = useApp((s) => s.pinnedItems);
  const now = useApp((s) => s.now);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.shiftKey || e.altKey) return;
      if (e.key === "1") { e.preventDefault(); setView("history"); }
      else if (e.key === "2") { e.preventDefault(); setView("devices"); }
      else if (e.key === "3") { e.preventDefault(); setView("transfers"); }
      else if (e.key === "4") { e.preventDefault(); setView("settings"); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setView]);

  const connected = peers.filter((d) => d.state === "connected").length;
  const statusLabel = sync.paused
    ? t("nav.statusPaused")
    : !sync.online
      ? t("nav.statusStarting")
      : connected === 0
        ? (sync.queuedCount > 0 ? t("nav.statusReconnecting") : t("nav.statusNoneConnected"))
        : t("nav.statusSynced", { n: connected + 1 });
  // Sem nenhum pareado não há o que sincronizar; o status só confundiria.
  const showStatus = peers.length > 0 || sync.paused;
  const panelShortcut = useApp((s) => s.settings?.shortcuts.openPanel);
  const statusClass = sync.paused ? "" : connected === 0 ? (sync.queuedCount > 0 ? "warn" : "") : "online";

  return (
    <div className="win">
      <div className="titlebar" onMouseDown={(e) => { if (e.button === 0 && (e.target as HTMLElement).closest(".tb-btn") === null) void startDragging(); }} onDoubleClick={() => api.windowAction("toggle_maximize")}>
        <div className="brand-dot" />
        <span className="title">ClipSync</span>
        <span className="subtitle">— {me?.name}</span>
        <div className="grow" />
        <div className="row-flex">
          <button className="tb-btn" title={t("nav.minimize")} onClick={() => api.windowAction("minimize")}>
            <svg width="12" height="12" viewBox="0 0 12 12" stroke="currentColor" strokeWidth="1.2"><path d="M1 6h10" /></svg>
          </button>
          <button className="tb-btn" title={t("nav.maximize")} onClick={() => api.windowAction("toggle_maximize")}>
            <svg width="12" height="12" viewBox="0 0 12 12" stroke="currentColor" strokeWidth="1.2" fill="none"><rect x="1.5" y="1.5" width="9" height="9" /></svg>
          </button>
          <button className="tb-btn close" title={t("nav.close")} onClick={() => api.windowAction("close")}>
            <svg width="12" height="12" viewBox="0 0 12 12" stroke="currentColor" strokeWidth="1.2"><path d="M1.5 1.5l9 9M10.5 1.5l-9 9" /></svg>
          </button>
        </div>
      </div>

      <div className="win-body">
        <nav className="sidebar">
          {NAV.map(({ id, label, hint, Icon }) => (
            <button key={id} className={`nav-item${view === id ? " active" : ""}`} onClick={() => setView(id)}>
              <Icon size={17} />
              {t(label)}
              <span className="hint">{id === "pinned" ? pinned : hint}</span>
            </button>
          ))}

          <div className="section-title">{t("nav.devices")}</div>
          {me && (
            <div className="side-dev">
              <span className="dot online" />
              <span className="name">{me.name}</span>
              <span className="when">{t("nav.thisPc")}</span>
            </div>
          )}
          {peers.map((d) => (
            <div className="side-dev" key={d.deviceId}>
              <Dot device={d} />
              <span className="name">{d.name}</span>
              <span className="when">{d.state === "connected" ? t("time.now") : d.state === "connecting" ? "…" : d.lastSeen ? shortTime(d.lastSeen, now) : t("device.offlineLower")}</span>
            </div>
          ))}

          <div className="grow" />
          <div className="side-status">
            {showStatus && (
              <div className="row-flex" style={{ gap: 8 }}>
                <span className={`dot ${statusClass}`} />
                <span className="label">{statusLabel}</span>
              </div>
            )}
            {panelShortcut && <span className="hint">{t("nav.quickPanel", { key: panelShortcut })}</span>}
          </div>
        </nav>

        {view === "history" || view === "pinned" ? <HistoryView /> : view === "devices" ? <DevicesView /> : view === "transfers" ? <TransfersView /> : <SettingsView />}
      </div>
      <div className="float-stack">
        <AskPrompt variant="float" />
        <CopiedFilesOffer variant="float" />
      </div>
      <PairPrompts />
      <NetworkWarning />
      <Toast />
    </div>
  );
}
