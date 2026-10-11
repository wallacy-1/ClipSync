import { useEffect, useMemo, useRef, useState } from "react";
import { useT, type Key } from "../i18n";
import { Clipboard, Copy, External, Mail, Pause, Play, Search, Send, Trash, WifiOff } from "../components/Icons";
import { PAUSE_OPTIONS, pausedUntilText } from "../components/pause";
import { SHORTCUTS } from "../components/SettingsRows";
import { CheckBox, Confirm, Dot, ItemImage, ItemText, StateTag, TypeTag, useMasked } from "../components/ui";
import { TYPE_LABEL, countdown, deviceName, imageLabel, isMono, isOpenable, relTime } from "../lib/format";
import type { Filter } from "../lib/types";
import { usePeers, useVisibleItems, useApp } from "../store/app";

const FILTERS: { id: Filter; label: Key }[] = [
  { id: "all", label: "history.filterAll" },
  { id: "text", label: "history.filterText" },
  { id: "code", label: "history.filterCode" },
  { id: "link", label: "history.filterLink" },
  { id: "email", label: "history.filterEmail" },
  { id: "image", label: "history.filterImage" },
  { id: "sensitive", label: "history.filterSensitive" },
];

export function HistoryView() {
  const t = useT();
  const view = useApp((s) => s.view);
  const query = useApp((s) => s.query);
  const setQuery = useApp((s) => s.setQuery);
  const filter = useApp((s) => s.filter);
  const setFilter = useApp((s) => s.setFilter);
  const items = useVisibleItems();
  const total = useApp((s) => s.totalItems);
  const pinned = useApp((s) => s.pinnedItems);
  const sync = useApp((s) => s.sync);
  const peers = usePeers();
  const selectedId = useApp((s) => s.selectedId);
  const select = useApp((s) => s.select);
  const setPaused = useApp((s) => s.setPaused);
  const retry = useApp((s) => s.retry);
  const clearHistory = useApp((s) => s.clearHistory);
  const searchRef = useRef<HTMLInputElement>(null);
  const [pauseMenu, setPauseMenu] = useState(false);
  const pauseRef = useRef<HTMLDivElement>(null);
  const [confirmClear, setConfirmClear] = useState(false);

  // Menu de pausa: fecha com Esc, clique fora ou quando a pausa muda por outro caminho.
  useEffect(() => {
    if (!pauseMenu) return;
    const onDoc = (e: MouseEvent) => { if (!pauseRef.current?.contains(e.target as Node)) setPauseMenu(false); };
    const onEsc = (e: KeyboardEvent) => { if (e.key === "Escape") setPauseMenu(false); };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onEsc);
    return () => { document.removeEventListener("mousedown", onDoc); document.removeEventListener("keydown", onEsc); };
  }, [pauseMenu]);
  useEffect(() => { if (sync.paused) setPauseMenu(false); }, [sync.paused]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === "f") { e.preventDefault(); searchRef.current?.focus(); }
      // Ctrl+Enter abre o link/e-mail selecionado (a menos que esteja mascarado).
      if (e.ctrlKey && e.key === "Enter") {
        const st = useApp.getState();
        const it = st.items.find((i) => i.id === st.selectedId);
        if (it && isOpenable(it) && !(it.isSensitive && (st.settings?.hideSensitive ?? true) && !st.revealed[it.id])) {
          e.preventDefault();
          void st.openItem(it.id);
        }
      }
      if (e.key === "Escape" && document.activeElement === searchRef.current) { setQuery(""); searchRef.current?.blur(); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setQuery]);

  const selected = items.find((i) => i.id === selectedId) ?? items[0] ?? null;
  useEffect(() => {
    if (selected && selected.id !== selectedId) select(selected.id);
  }, [selected, selectedId, select]);

  const connected = peers.filter((d) => d.state === "connected");
  const offlineQueued = sync.queuedCount > 0 && connected.length === 0 && peers.length > 0;
  const queuedFor = peers.filter((d) => d.queued > 0).map((d) => d.name);

  const count = query.trim()
    ? t("history.results", { n: items.length, total })
    : view === "pinned"
      ? t("history.pinnedCount", { n: pinned })
      : total === 0
        ? t("history.noItems")
        : t("history.totalCount", { n: total, pinned: t("history.pinnedShort", { n: pinned }) });

  return (
    <>
      <div className="main">
        <div className="toolbar">
          <div className="toolbar-row">
            <label className="search">
              <Search size={16} style={{ color: "var(--color-neutral-600)" }} />
              <input ref={searchRef} value={query} onChange={(e) => setQuery(e.target.value)} placeholder={t("history.searchPlaceholder")} />
              <span className="kbd">Ctrl+F</span>
            </label>
            <button className="btn btn-secondary" disabled={total === 0} onClick={() => setConfirmClear(true)}>
              <Trash size={15} /> {t("history.clearHistory")}
            </button>
            {sync.paused ? (
              <button className="btn btn-primary" onClick={() => setPaused(false)}>
                <Play size={15} /> {t("history.resumeSync")}
              </button>
            ) : peers.length > 0 && (
              <div className="menu-wrap" ref={pauseRef}>
                <button className="btn btn-primary" aria-haspopup="menu" aria-expanded={pauseMenu} onClick={() => setPauseMenu((m) => !m)}>
                  <Pause size={15} /> {t("history.pauseSync")}
                </button>
                {pauseMenu && (
                  <div className="tr-menu pause-menu" role="menu">
                    {PAUSE_OPTIONS.map((o) => (
                      <button key={o.label} role="menuitem" onClick={() => { setPauseMenu(false); void setPaused(true, o.minutes); }}>{o.label}</button>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
          {view !== "pinned" && (
            <div className="filters">
              {FILTERS.map((f) => (
                <button key={f.id} className={`chip${filter === f.id ? " active" : ""}`} onClick={() => setFilter(f.id)}>{t(f.label)}</button>
              ))}
              <span className="count">{count}</span>
            </div>
          )}
          {view === "pinned" && (
            <div className="filters"><span className="chip active">{t("nav.pinned")}</span><span className="count">{count}</span></div>
          )}
        </div>

        {sync.paused && (
          <div className="banner paused">
            <Pause size={18} />
            <span>{sync.pausedUntil ? t("history.pausedUntilBanner", { until: pausedUntilText(sync.pausedUntil) }) : t("history.pausedBanner")}</span>
            <button className="btn btn-ghost" onClick={() => setPaused(false)}>{t("common.resume")}</button>
          </div>
        )}
        {!sync.paused && offlineQueued && (
          <div className="banner offline">
            <WifiOff size={18} style={{ color: "var(--color-accent-800)" }} />
            <span>
              {t("history.offlineBanner", {
                who: queuedFor.length ? queuedFor.join(", ") : t("history.yourDevices"),
                queue: t("history.queueItems", { n: sync.queuedCount }),
              })}
            </span>
            <button className="btn btn-ghost" onClick={retry}>{t("common.retryNow")}</button>
          </div>
        )}

        {items.length === 0 ? (
          <div className="empty">
            <div className="orb"><Clipboard size={40} /></div>
            <h3>{query.trim() ? t("history.emptySearchTitle") : view === "pinned" ? t("history.emptyPinnedTitle") : t("history.emptyTitle")}</h3>
            <p>
              {query.trim()
                ? t("history.emptySearchBody")
                : view === "pinned"
                  ? t("history.emptyPinnedBody")
                  : t("history.emptyBody")}
            </p>
            {!query.trim() && view !== "pinned" && (
              <button className="btn btn-secondary" onClick={() => useApp.getState().setView("devices")}>{t("history.howItWorks")}</button>
            )}
          </div>
        ) : (
          <div className="list">
            {items.map((it) => <Row key={it.id} id={it.id} selected={selected?.id === it.id} />)}
          </div>
        )}
      </div>

      <Rail />
      {confirmClear && (
        <Confirm
          title={t("settings.clearTitle")}
          body={t("settings.clearBody")}
          confirmLabel={t("settings.clear")}
          danger
          onCancel={() => setConfirmClear(false)}
          onConfirm={() => { setConfirmClear(false); void clearHistory(true); }}
        />
      )}
    </>
  );
}

function Row({ id, selected }: { id: string; selected: boolean }) {
  const t = useT();
  const it = useApp((s) => s.items.find((i) => i.id === id))!;
  const devices = useApp((s) => s.devices);
  const me = useApp((s) => s.me);
  const now = useApp((s) => s.now);
  const revealed = useApp((s) => !!s.revealed[id]);
  const hideSensitive = useApp((s) => s.settings?.hideSensitive ?? true);
  const hasPeers = usePeers().length > 0;
  const { select, copy, remove, reveal, hide, send, sendTargets } = useApp.getState();
  if (!it) return null;
  const masked = it.isSensitive && hideSensitive && !revealed;
  const note = it.isSensitive
    ? `${t(masked ? "item.hidden" : "item.visible")} · ${t("item.expiresIn", { time: countdown(it.expiresAt, now) || "—" })}`
    : it.isPinned ? t("item.pinned") : "";

  return (
    <div className={`row${selected ? " selected" : ""}`} onClick={() => select(it.id)} onDoubleClick={() => copy(it.id)}>
      <div className="kind"><TypeTag item={it} /></div>
      <div className="body">
        <ItemText item={it} />
        <div className="meta">
          <span>{deviceName(devices, it.sourceDeviceId, me?.deviceId ?? "")}</span>
          <span>·</span>
          <span>{relTime(it.createdAt, now)}</span>
          {note && <span className="note">· {note}</span>}
        </div>
      </div>
      {it.isSensitive && hideSensitive && (
        <button className="btn btn-secondary sm" onClick={(e) => { e.stopPropagation(); masked ? void reveal(it.id) : hide(it.id); }}>
          {t(masked ? "history.reveal" : "history.hide")}
        </button>
      )}
      {hasPeers && <div className="state"><StateTag item={it} /></div>}
      <div className="actions">
        <button className="icon-btn" title={t("history.copy")} onClick={(e) => { e.stopPropagation(); void copy(it.id); }}><Copy size={15} /></button>
        {hasPeers && <button className="icon-btn" title={t("history.sendToMarked")} onClick={(e) => { e.stopPropagation(); void send(it.id, sendTargets); }}><Send size={15} /></button>}
        <button className="icon-btn" title={t("history.delete")} onClick={(e) => { e.stopPropagation(); void remove(it.id); }}><Trash size={15} /></button>
      </div>
    </div>
  );
}

function Rail() {
  const t = useT();
  const selectedId = useApp((s) => s.selectedId);
  const item = useApp((s) => s.items.find((i) => i.id === selectedId) ?? null);
  const devices = useApp((s) => s.devices);
  const peers = usePeers();
  const me = useApp((s) => s.me);
  const now = useApp((s) => s.now);
  const sendTargets = useApp((s) => s.sendTargets);
  const toggleTarget = useApp((s) => s.toggleTarget);
  const shortcuts = useApp((s) => s.settings?.shortcuts);
  const { copy, pin, send, openItem, revealFile } = useApp.getState();
  const { masked, text } = useMasked(item);

  const syncedWith = useMemo(() => {
    if (!item) return "";
    if (item.sourceDeviceId !== me?.deviceId) return t("history.received");
    if (item.syncState === "synced") return t("history.devicesCount", { n: peers.filter((d) => d.autoSend).length });
    if (item.syncState === "queued") return t("history.queued");
    if (item.syncState === "sending") return t("history.sending");
    return t("history.onlyHere");
  }, [item, me, peers, t]);

  return (
    <aside className="rail">
      <div className="section-title">{t("history.selectedItem")}</div>
      {item ? (
        <>
          {item.contentType === "image" ? (
            <div className="content img">
              <ItemImage item={item} full />
              <span>{imageLabel(item)}</span>
            </div>
          ) : (
            <div className={`content${isMono(item.contentType) ? "" : " text"}`}>{text}</div>
          )}
          <div className="kv">
            <div><span>{t("history.origin")}</span><span>{deviceName(devices, item.sourceDeviceId, me?.deviceId ?? "")}</span></div>
            <div><span>{t("history.copiedAt")}</span><span>{relTime(item.createdAt, now)}</span></div>
            <div><span>{t("history.kind")}</span><span>{TYPE_LABEL[item.contentType]}</span></div>
            {item.expiresAt && <div><span>{t("history.expires")}</span><span>{t("history.expiresIn", { time: countdown(item.expiresAt, now) })}</span></div>}
            {peers.length > 0 && <div><span>{t("history.syncedWith")}</span><span>{syncedWith}</span></div>}
          </div>
          <div className="actions">
            <button className="btn btn-primary btn-block" onClick={() => copy(item.id)}>{t("history.copyNow")}</button>
            {isOpenable(item) && (
              <button className="btn btn-secondary btn-block" disabled={masked} onClick={() => openItem(item.id)}>
                {item.contentType === "email" ? <Mail size={15} /> : <External size={15} />} {item.contentType === "email" ? t("history.writeEmail") : t("history.openLink")}
              </button>
            )}
            {item.contentType === "image" && (
              <button className="btn btn-secondary btn-block" onClick={() => revealFile(item.id)}>{t("common.showInFolder")}</button>
            )}
            <div className="pair">
              <button className="btn btn-secondary" onClick={() => pin(item.id, !item.isPinned)}>{item.isPinned ? t("history.unpin") : t("history.pin")}</button>
              {peers.length > 0 && <button className="btn btn-secondary" disabled={masked} onClick={() => send(item.id, sendTargets)}>{t("history.send")}</button>}
            </div>
          </div>
        </>
      ) : (
        <div className="muted" style={{ fontSize: 13.5 }}>{t("history.selectHint")}</div>
      )}

      {peers.length > 0 && <div className="targets">
        <div className="section-title">{t("history.sendTo")}</div>
        {peers.map((d) => (
          <div className="target-row" key={d.deviceId}>
            <Dot device={d} />
            <span className="name">{d.name}</span>
            <CheckBox small on={sendTargets.includes(d.deviceId)} onChange={() => toggleTarget(d.deviceId)} disabled={d.state === "blocked"} />
          </div>
        ))}
      </div>}

      <div className="shortcuts">
        <div className="section-title">{t("history.shortcuts")}</div>
        <div className="lines">
          {SHORTCUTS.map((sc) => (
            <div key={sc.id}><span>{sc.label}</span><span>{shortcuts?.[sc.id] || "—"}</span></div>
          ))}
        </div>
      </div>
    </aside>
  );
}
