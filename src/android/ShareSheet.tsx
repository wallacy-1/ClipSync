import { useEffect, useState } from "react";
import { useT } from "../i18n";
import { Shield } from "../components/Icons";
import { Avatar } from "../components/ui";
import { bytes } from "../lib/format";
import { useApp } from "../store/app";
import { useFileTargets } from "../components/transfer";

/** "Compartilhar → ClipSync": escolher para qual dispositivo vai o que outro app compartilhou. */
export function ShareSheet() {
  const t = useT();
  const share = useApp((s) => s.pendingShare);
  const sendShare = useApp((s) => s.sendShare);
  const dismiss = useApp((s) => s.dismissShare);
  const { targets, loading } = useFileTargets();
  const [selected, setSelected] = useState<string | null>(null);
  const [sending, setSending] = useState(false);

  // Offline também serve: arquivo e texto entram na fila e saem ao reconectar.
  const options = targets.map((o) => ({ ...o, selectable: o.selectable || o.cap === "offline" }));
  const available = options.filter((o) => o.selectable);
  useEffect(() => {
    if (selected === null && available.length > 0) setSelected(available[0].device.deviceId);
  }, [available, selected]);
  if (!share) return null;

  const chosen = options.find((o) => o.device.deviceId === selected);
  const files = share.files;
  const total = files.reduce((n, f) => n + Math.max(0, f.size), 0);
  const title = files.length > 0 ? t("android.share.titleFiles", { n: files.length }) : t("android.share.titleText");
  const preview = files.length === 0
    ? share.text
    : files.length === 1
      ? `${files[0].name}${files[0].size > 0 ? ` · ${bytes(files[0].size)}` : ""}`
      : `${files.slice(0, 3).map((f) => f.name).join(", ")}${files.length > 3 ? ` ${t("android.share.andMore", { n: files.length - 3 })}` : ""}${total > 0 ? ` · ${bytes(total)}` : ""}`;

  return (
    <div className="sheet-backdrop" onClick={dismiss}>
      <div className="sheet" onClick={(e) => e.stopPropagation()}>
        <div className="handle" />
        <h3>{title}</h3>
        <div className="sub">{t("android.share.sub")}</div>
        <div className={`preview${files.length === 0 ? " body" : ""}`}>{preview}</div>
        <div className="targets">
          {!loading && available.length === 0 && (
            <div className="muted" style={{ fontSize: 13.5 }}>{t("android.share.none")}</div>
          )}
          {options.map((o) => (
            <button
              key={o.device.deviceId}
              className={`target-row2${selected === o.device.deviceId ? " selected" : ""}${!o.selectable ? " blocked" : ""}`}
              disabled={!o.selectable}
              onClick={() => setSelected(o.device.deviceId)}
            >
              <Avatar device={o.device} />
              <div className="info">
                <div className="name">{o.device.name}</div>
                <div className="detail">{o.cap === "offline" ? t("copiedOffer.offlineHint") : o.detail}</div>
              </div>
              {o.selectable ? <span className={`radio${selected === o.device.deviceId ? " on" : ""}`} /> : <span className="tag tag-queued">{o.capLabel}</span>}
            </button>
          ))}
        </div>
        <button
          className="btn btn-primary btn-block cta"
          disabled={!chosen?.selectable || sending}
          onClick={async () => {
            if (!chosen) return;
            setSending(true);
            await sendShare(chosen.device.deviceId);
            setSending(false);
          }}
        >
          {chosen ? t("sendFile.sendTo", { name: chosen.device.name }) : t("common.send")}
        </button>
        <div className="foot"><Shield size={15} /> {files.length > 0 ? t("android.share.footFiles", { files: t("files.files", { n: files.length }) }) : t("android.share.foot")}</div>
      </div>
    </div>
  );
}
