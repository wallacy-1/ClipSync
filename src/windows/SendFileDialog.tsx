import { useEffect, useState } from "react";
import { File, MonitorOff, Shield } from "../components/Icons";
import { useT } from "../i18n";
import { useFileTargets } from "../components/transfer";
import { Avatar } from "../components/ui";
import { filesSummary } from "../lib/format";
import type { LocalFile } from "../lib/types";
import { useApp } from "../store/app";

export function SendFileDialog({ files, onClose }: { files: LocalFile[]; onClose: () => void }) {
  const t = useT();
  const { targets, loading } = useFileTargets();
  const sendFiles = useApp((s) => s.sendFiles);
  const summary = filesSummary(files);
  const setView = useApp((s) => s.setView);
  const [selected, setSelected] = useState<string | null>(null);
  const [sending, setSending] = useState(false);

  const available = targets.filter((x) => x.selectable);
  useEffect(() => {
    if (selected === null && available.length > 0) setSelected(available[0].device.deviceId);
  }, [available, selected]);

  const chosen = targets.find((x) => x.device.deviceId === selected);
  const none = !loading && available.length === 0;

  return (
    <div className="dialog-backdrop" onClick={onClose}>
      <div className="dialog sendfile" onClick={(e) => e.stopPropagation()}>
        <div className="head">
          <div className="tr-icon"><File size={20} /></div>
          <div style={{ minWidth: 0 }}>
            <h3>{files.length === 1 && !files[0].dir ? t("sendFile.titleFile") : t("sendFile.title")}</h3>
            <div className="file" title={files.map((f) => f.path).join("\n")}>{summary.title}{summary.meta ? ` · ${summary.meta}` : ""}</div>
          </div>
        </div>

        {none ? (
          <div className="none">
            <div className="orb"><MonitorOff size={32} /></div>
            <h3>{t("sendFile.noneTitle")}</h3>
            <p>{t("sendFile.noneBody")}</p>
            <div className="btns">
              <button className="btn btn-secondary" onClick={() => { onClose(); setView("devices"); }}>{t("sendFile.seeDevices")}</button>
              <button className="btn btn-primary" onClick={onClose}>{t("sendFile.tryAgain")}</button>
            </div>
          </div>
        ) : (
          <div>
            <div className="section-title">{t("sendFile.destination")}</div>
            <div className="targets">
              {targets.map((tg) => (
                <button
                  key={tg.device.deviceId}
                  className={`target-row2${selected === tg.device.deviceId ? " selected" : ""}${!tg.selectable ? " blocked" : ""}`}
                  disabled={!tg.selectable}
                  onClick={() => setSelected(tg.device.deviceId)}
                >
                  <Avatar device={tg.device} />
                  <div className="info">
                    <div className="name">{tg.device.name}</div>
                    <div className="detail">{tg.detail}</div>
                  </div>
                  {tg.cap ? <span className="tag tag-queued">{tg.capLabel}</span> : <span className={`radio${selected === tg.device.deviceId ? " on" : ""}`} />}
                </button>
              ))}
              {loading && targets.length === 0 && <div className="muted" style={{ fontSize: 13.5 }}>{t("sendFile.searching")}</div>}
            </div>
            <div className="note"><Shield size={15} /> {t("sendFile.note")}</div>
            <div className="actions">
              <button className="btn btn-secondary" onClick={onClose}>{t("common.cancel")}</button>
              <button
                className="btn btn-primary"
                disabled={!chosen?.selectable || sending}
                onClick={async () => {
                  if (!chosen) return;
                  setSending(true);
                  await sendFiles(chosen.device.deviceId, files);
                  setSending(false);
                  onClose();
                }}
              >
                {chosen ? t("sendFile.sendTo", { name: chosen.device.name }) : t("sendFile.title")}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
