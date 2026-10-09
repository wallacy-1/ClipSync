import { useState } from "react";
import { locale, useT } from "../i18n";
import { Clipboard, ClipboardCheck, Copy, External, Mail, Pause, Pin, Search, Send } from "../components/Icons";
import { ItemImage, ItemText, TypeTag, useMasked } from "../components/ui";
import { pausedUntilText } from "../components/pause";
import { TYPE_LABEL, countdown, deviceName, imageLabel, isMono, isOpenable, relTime } from "../lib/format";
import { usePeers, useApp } from "../store/app";
import type { ViewCtx } from "./AndroidShell";
import { PauseSheet } from "./PauseSheet";

export function HomeView({ openSend, openDetail, goTab }: ViewCtx) {
  const t = useT();
  const items = useApp((s) => s.items);
  const devices = useApp((s) => s.devices);
  const peers = usePeers();
  const me = useApp((s) => s.me);
  const sync = useApp((s) => s.sync);
  const now = useApp((s) => s.now);
  const { copy, pin, reveal, hide, setQuery, setPaused, openItem } = useApp.getState();
  const current = items[0] ?? null;
  const rest = items.slice(1, 6);
  const { masked, text } = useMasked(current);
  const [holding, setHolding] = useState(false);
  const [pauseOpen, setPauseOpen] = useState(false);

  const connected = peers.filter((d) => d.state === "connected").length;
  const sending = items.some((i) => i.syncState === "sending");
  const status = sync.paused
    ? { cls: "tag-queued", label: t("tray.paused") }
    : sending
      ? { cls: "tag-sending", label: t("android.home.statusSending") }
      : connected > 0
        ? { cls: "tag-synced", label: t("tray.synced") }
        : peers.length === 0
          ? { cls: "", label: t("android.home.statusNoPeer") }
          : { cls: "tag-queued", label: t("device.offline") };

  const currentMeta = current
    ? current.syncState === "sending"
      ? t("android.home.copiedSending", { when: relTime(current.createdAt, now) })
      : t("android.home.copied", { when: relTime(current.createdAt, now), device: deviceName(devices, current.sourceDeviceId, me?.deviceId ?? "") })
    : "";
  const currentNote = current?.isSensitive
    ? t(masked ? "android.home.sensitiveHold" : "android.home.sensitiveVisible", { type: TYPE_LABEL[current.contentType], time: countdown(current.expiresAt, now) || "—" })
    : current?.syncState === "sending"
      ? t("android.home.sendingTo", { names: new Intl.ListFormat(locale(), { type: "conjunction" }).format(peers.filter((d) => d.autoSend).map((d) => d.name)) })
      : current?.syncState === "queued"
        ? t("android.home.queuedNote")
        : "";

  // Toque e segure revela o item sensível enquanto o dedo estiver no cartão.
  const holdProps = current?.isSensitive
    ? {
        onPointerDown: () => { setHolding(true); void reveal(current.id); },
        onPointerUp: () => { setHolding(false); hide(current.id); },
        onPointerLeave: () => { if (holding) { setHolding(false); hide(current.id); } },
        onContextMenu: (e: React.MouseEvent) => e.preventDefault(),
      }
    : {};

  return (
    <>
      <div className="and-head">
        <div className="logo"><Clipboard size={17} /></div>
        <span className="brand">ClipSync</span>
        {(peers.length > 0 || sync.paused) && (
          <button className={`tag ${status.cls}`} onClick={() => setPauseOpen(true)}>{status.label}</button>
        )}
        <button className="icon-btn lg" onClick={() => { setQuery(""); goTab("history"); }}><Search size={20} /></button>
      </div>

      {sync.paused && (
        <div className="banner paused">
          <Pause size={18} />
          <span>{t("android.home.pausedBanner", { until: pausedUntilText(sync.pausedUntil) })}</span>
          <button className="btn btn-ghost" onClick={() => setPaused(false)}>{t("common.resume")}</button>
        </div>
      )}

      <div className="and-scroll">
        <div className="and-section">
          <div className="section-title">{t("android.home.current")}</div>
          {current ? (
            <div className="current">
              <div className="head">
                {current.isSensitive ? <span className="tag tag-sensitive">{t("item.sensitive")}</span> : <TypeTag item={current} />}
                <span>{currentMeta}</span>
              </div>
              {current.contentType === "image" ? (
                <div className="text body img">
                  <ItemImage item={current} full />
                  <span>{imageLabel(current)}</span>
                </div>
              ) : (
                <div className={`text${isMono(current.contentType) ? "" : " body"}${masked ? " dim" : ""}`} {...holdProps}>{text}</div>
              )}
              {currentNote && <div className="note">{currentNote}</div>}
              <div className="actions">
                {peers.length > 0 && (
                  <button className="btn btn-primary" disabled={masked} onClick={() => openSend(current.id)}>
                    <Send size={17} /> {t("common.send")}
                  </button>
                )}
                {isOpenable(current) && (
                  <button className="btn btn-secondary" aria-label={current.contentType === "email" ? t("history.writeEmail") : t("common.open")} disabled={masked} onClick={() => openItem(current.id)}>
                    {current.contentType === "email" ? <Mail size={19} /> : <External size={19} />}
                  </button>
                )}
                <button className="btn btn-secondary" aria-label={t("history.copy")} onClick={() => copy(current.id)}><Copy size={19} /></button>
                <button className={`btn btn-secondary${current.isPinned ? " on" : ""}`} aria-label={t("history.pin")} onClick={() => pin(current.id, !current.isPinned)}><Pin size={19} /></button>
              </div>
            </div>
          ) : (
            <div className="current">
              <div className="head"><span className="tag">{t("android.home.emptyTag")}</span><span>{t("android.home.emptyMeta")}</span></div>
              <div className="text body muted">{t("android.home.emptyBody")}</div>
            </div>
          )}
        </div>

        <div className="and-section-head">
          <span className="section-title">{t("android.home.history")}</span>
          <button className="link" onClick={() => goTab("history")}>{t("android.home.seeAll")}</button>
        </div>

        {rest.length === 0 ? (
          <div className="and-empty">
            <div className="orb"><ClipboardCheck size={36} /></div>
            <h3>{t("android.history.emptyTitle")}</h3>
            <p>{t("android.history.emptyBody")}</p>
          </div>
        ) : (
          <div className="and-list">
            {rest.map((it) => (
              <div className="and-row" key={it.id} onClick={() => openDetail(it.id)}>
                <div className="body">
                  <ItemText item={it} />
                  <div className="meta">
                    <TypeTag item={it} />
                    <span>{deviceName(devices, it.sourceDeviceId, me?.deviceId ?? "")}</span><span>·</span><span>{relTime(it.createdAt, now)}</span>
                  </div>
                </div>
                <button className="icon-btn lg filled" aria-label={t("history.copy")} onClick={(e) => { e.stopPropagation(); void copy(it.id); }}><Copy size={18} /></button>
              </div>
            ))}
          </div>
        )}
      </div>
      {pauseOpen && <PauseSheet onClose={() => setPauseOpen(false)} />}
    </>
  );
}
