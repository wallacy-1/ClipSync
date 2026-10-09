import { useEffect, useMemo, useState } from "react";
import { useT } from "../i18n";
import { Bell, File, Shield, Transfers, Upload } from "../components/Icons";
import { Avatar } from "../components/ui";
import { filesSummary, isClipImage, isHiddenTransfer, isTransferActive, transferDetail, TRANSFER_LABEL, TRANSFER_TAG } from "../lib/format";
import type { LocalFile, TransferRecord } from "../lib/types";
import { useApp } from "../store/app";
import { useFileTargets, useTransferRow } from "../components/transfer";

export function TransfersView() {
  const t = useT();
  const all = useApp((s) => s.transfers);
  const transfers = useMemo(() => all.filter((x) => !isHiddenTransfer(x)), [all]);
  const loaded = useApp((s) => s.transfersLoaded);
  const refresh = useApp((s) => s.refreshTransfers);
  const pickFile = useApp((s) => s.pickFile);
  const pendingFiles = useApp((s) => s.pendingFiles);
  const setPendingFiles = useApp((s) => s.setPendingFiles);
  const clearFinished = useApp((s) => s.clearFinishedTransfers);

  useEffect(() => {
    if (!loaded) void refresh();
  }, [loaded, refresh]);

  const active = transfers.filter(isTransferActive).length;
  const finished = transfers.length - active;
  const failed = transfers.filter((x) => x.state.kind === "failed").length;
  const waiting = transfers.filter((x) => x.state.kind === "waiting").length;
  const pill = failed > 0
    ? { cls: "tag-failed", label: t("android.transfers.failures", { n: failed }) }
    : waiting > 0 && active === waiting
      ? { cls: "tag-sending", label: t("android.transfers.reconnecting") }
      : active > 0
        ? { cls: "tag-synced", label: t("android.transfers.active", { n: active }) }
        : { cls: "tag-synced", label: t("android.transfers.idle") };

  return (
    <>
      <div className="and-head" style={{ padding: "16px 18px 12px", gap: 12 }}>
        <span className="page-title">{t("nav.transfers")}</span>
        <span className={`tag pill ${pill.cls}`}>{pill.label}</span>
      </div>

      <div className="and-scroll">
        <NotifyBanner />
        {loaded && transfers.length === 0 ? (
          <div className="and-empty">
            <div className="orb"><Transfers size={36} /></div>
            <h3>{t("transfers.emptyTitle")}</h3>
            <p style={{ lineHeight: 1.5 }}>{t("android.transfers.emptyBody")}</p>
          </div>
        ) : (
          <div className="and-tr-list">
            {finished > 0 && <button className="btn btn-ghost and-clear" onClick={() => void clearFinished()}>{t("transfers.clearFinished")}</button>}
            {transfers.map((x) => <TransferCard key={x.id} rec={x} />)}
          </div>
        )}
      </div>

      <div className="and-tr-cta">
        <button className="btn btn-primary btn-block" onClick={() => void pickFile()}><Upload size={18} /> {t("transfers.sendFile")}</button>
      </div>

      {pendingFiles && <SendFileSheet files={pendingFiles} onClose={() => setPendingFiles(null)} />}
    </>
  );
}

/** Android 13+ sem permissão de notificação: o FGS roda, mas progresso e Cancelar somem. */
function NotifyBanner() {
  const t = useT();
  const perm = useApp((s) => s.notifyPerm);
  const request = useApp((s) => s.requestNotifyPerm);
  if (perm !== "prompt" && perm !== "blocked") return null;
  return (
    <div className="and-notice">
      <Bell size={18} />
      <div className="body">
        <div className="title">{t("android.transfers.notifOff")}</div>
        <div className="text">{t("android.transfers.notifBody")}</div>
      </div>
      <button className="btn btn-secondary" onClick={() => void request()}>{perm === "blocked" ? t("android.transfers.openSettings") : t("android.transfers.allow")}</button>
    </div>
  );
}

function TransferCard({ rec }: { rec: TransferRecord }) {
  const t = useT();
  // Imagem do clipboard fica em disco privado: não abre por fora.
  const { kind, dev, incoming, hasBar, pct, primary, asked, accept, decline } = useTransferRow(rec, !isClipImage(rec));

  return (
    <div className="and-tr-card">
      <div className="top">
        <div className={`tr-icon${incoming ? " in" : ""}${kind === "failed" ? " failed" : ""}`}><File size={18} /></div>
        <div className="body">
          <div className="tr-name">{rec.safeName}</div>
          <div className="tr-meta">
            <span className={`tr-dir${incoming ? " in" : ""}`}>{incoming ? t("transfers.receiving") : t("transfers.sending")}</span>
            <span>{dev}</span>
            {isClipImage(rec) && <span>· {t("transfers.clipImage")}</span>}
          </div>
        </div>
        <span className={`tag ${TRANSFER_TAG[kind]}`}>{TRANSFER_LABEL[kind]}</span>
      </div>
      {hasBar && <div className={`tr-bar ${kind}`}><span style={{ width: `${pct}%` }} /></div>}
      <div className="foot">
        <span className={`tr-detail${kind === "failed" ? " failed" : ""}`}>{transferDetail(rec, dev)}</span>
        {asked ? (
          <div className="row-flex" style={{ gap: 6 }}>
            <button className="btn btn-secondary" onClick={decline}>{t("common.decline")}</button>
            <button className="btn btn-primary" onClick={accept}>{t("common.accept")}</button>
          </div>
        ) : primary && <button className="btn btn-secondary" disabled={primary.disabled} onClick={primary.run}>{primary.label}</button>}
      </div>
    </div>
  );
}

function SendFileSheet({ files, onClose }: { files: LocalFile[]; onClose: () => void }) {
  const t = useT();
  const { targets, loading } = useFileTargets();
  const sendFiles = useApp((s) => s.sendFiles);
  const summary = filesSummary(files);
  const [selected, setSelected] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const available = targets.filter((o) => o.selectable);
  useEffect(() => {
    if (selected === null && available.length > 0) setSelected(available[0].device.deviceId);
  }, [available, selected]);
  const chosen = targets.find((o) => o.device.deviceId === selected);

  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" onClick={(e) => e.stopPropagation()}>
        <div className="handle" />
        <h3>{files.length === 1 ? t("sendFile.titleFile") : t("android.transfers.titleMany")}</h3>
        <div className="sub">{t(files.length === 1 ? "android.transfers.sheetOne" : "android.transfers.sheetMany")}</div>
        <div className="preview">{summary.title}{summary.meta ? ` · ${summary.meta}` : ""}</div>
        <div className="targets">
          {!loading && available.length === 0 && (
            <div className="muted" style={{ fontSize: 13.5 }}>{t("android.transfers.none")}</div>
          )}
          {targets.map((o) => (
            <button
              key={o.device.deviceId}
              className={`target-row2${selected === o.device.deviceId ? " selected" : ""}${!o.selectable ? " blocked" : ""}`}
              disabled={!o.selectable}
              onClick={() => setSelected(o.device.deviceId)}
            >
              <Avatar device={o.device} />
              <div className="info">
                <div className="name">{o.device.name}</div>
                <div className="detail">{o.detail}</div>
              </div>
              {o.cap ? <span className="tag tag-queued">{o.capLabel}</span> : <span className={`radio${selected === o.device.deviceId ? " on" : ""}`} />}
            </button>
          ))}
        </div>
        <button
          className="btn btn-primary btn-block cta"
          disabled={!chosen?.selectable || sending}
          onClick={async () => {
            if (!chosen) return;
            setSending(true);
            await sendFiles(chosen.device.deviceId, files);
            setSending(false);
            onClose();
          }}
        >
          {chosen ? t("sendFile.sendTo", { name: chosen.device.name }) : t("common.send")}
        </button>
        <div className="foot"><Shield size={15} /> {t("sendFile.note")}</div>
      </div>
    </div>
  );
}
