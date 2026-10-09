import { useState } from "react";
import { useT } from "../i18n";
import { Clock } from "../components/Icons";
import { Avatar, CheckBox, useMasked } from "../components/ui";
import { PLATFORM_LABEL, isMono } from "../lib/format";
import { usePeers, useApp } from "../store/app";

export function SendSheet({ itemId, onClose }: { itemId: string; onClose: () => void }) {
  const t = useT();
  const item = useApp((s) => s.items.find((i) => i.id === itemId) ?? null);
  const peers = usePeers();
  const settings = useApp((s) => s.settings);
  const send = useApp((s) => s.send);
  const [targets, setTargets] = useState<string[]>(peers.filter((d) => d.state === "connected" || d.autoSend).map((d) => d.deviceId));
  const { text } = useMasked(item);
  if (!item) return null;

  const toggle = (id: string) => setTargets((cur) => (cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id]));
  const hours = settings?.autoExpireHours;
  const expireText = item.isSensitive
    ? t("android.send.expireIn", { time: t("unit.min", { n: settings?.sensitiveExpireMinutes ?? 2 }) })
    : hours
      ? t("android.send.expireIn", { time: hours >= 24 ? t("android.send.days", { n: hours / 24 }) : t("android.send.hours", { n: hours }) })
      : t("android.send.keep");

  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" onClick={(e) => e.stopPropagation()}>
        <div className="handle" />
        <h3>{t("common.sendTo")}</h3>
        <div className="sub">{t("android.send.sub")}</div>
        <div className={`preview${isMono(item.contentType) ? "" : " body"}`}>{text}</div>
        <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
          {peers.length === 0 && <div className="muted" style={{ fontSize: 13.5 }}>{t("android.send.none")}</div>}
          {peers.map((d) => (
            <button className="target-card" key={d.deviceId} onClick={() => toggle(d.deviceId)} disabled={d.state === "blocked"}>
              <Avatar device={d} />
              <div className="info">
                <div className="name">{d.name}</div>
                <div className="detail">
                  {d.state === "connected" ? t("sendFile.detailConnected", { os: PLATFORM_LABEL[d.platform] ?? d.platform }) : d.state === "blocked" ? t("sendFile.capBlocked") : t("copiedOffer.offlineHint")}
                </div>
              </div>
              <CheckBox passive on={targets.includes(d.deviceId)} onChange={() => toggle(d.deviceId)} />
            </button>
          ))}
        </div>
        <button className="btn btn-primary btn-block cta" disabled={targets.length === 0} onClick={async () => { await send(item.id, targets); onClose(); }}>
          {t("android.send.toDevices", { n: targets.length })}
        </button>
        <div className="foot"><Clock size={15} /> {expireText}</div>
      </div>
    </div>
  );
}
