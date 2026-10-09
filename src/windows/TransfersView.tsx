import { useEffect, useMemo, useRef, useState } from "react";
import { useT } from "../i18n";
import { File, More, Transfers, Upload } from "../components/Icons";
import { useTransferRow } from "../components/transfer";
import { bytes, isClipImage, isHiddenTransfer, isTransferActive, transferDetail, TRANSFER_LABEL, TRANSFER_TAG } from "../lib/format";
import { onFileDrop } from "../lib/ipc";
import type { TransferRecord } from "../lib/types";
import { useApp } from "../store/app";
import { SendFileDialog } from "./SendFileDialog";

export function TransfersView() {
  const t = useT();
  const all = useApp((s) => s.transfers);
  const transfers = useMemo(() => all.filter((t) => !isHiddenTransfer(t)), [all]);
  const loaded = useApp((s) => s.transfersLoaded);
  const refresh = useApp((s) => s.refreshTransfers);
  const pickFile = useApp((s) => s.pickFile);
  const setPendingFiles = useApp((s) => s.setPendingFiles);
  const pendingFiles = useApp((s) => s.pendingFiles);
  const clearFinished = useApp((s) => s.clearFinishedTransfers);
  const [dragging, setDragging] = useState(false);

  useEffect(() => {
    if (!loaded) void refresh();
  }, [loaded, refresh]);

  // Arquivo solto sobre a janela: mesmo fluxo do botão, sem seletor.
  useEffect(() => {
    let off: (() => void) | undefined;
    void onFileDrop((paths) => {
      setDragging(false);
      if (paths.length === 0) return;
      setPendingFiles(paths.map((path) => ({ path, name: path.split(/[\\/]/).pop() ?? path, size: 0 })));
    }).then((u) => { off = u; });
    const onEnter = (e: DragEvent) => { e.preventDefault(); setDragging(true); };
    const onLeave = () => setDragging(false);
    window.addEventListener("dragover", onEnter);
    window.addEventListener("dragleave", onLeave);
    window.addEventListener("drop", onLeave);
    return () => {
      off?.();
      window.removeEventListener("dragover", onEnter);
      window.removeEventListener("dragleave", onLeave);
      window.removeEventListener("drop", onLeave);
    };
  }, [setPendingFiles]);

  const active = transfers.filter(isTransferActive).length;
  const finished = transfers.length - active;

  return (
    <div className="main">
      {dragging && <div className="drop-hint">{t("transfers.drop")}</div>}
      <div className="toolbar">
        <div className="page-head" style={{ margin: 0 }}>
          <h4>{t("transfers.title")}</h4>
          <span className="count">{transfers.length === 0 ? t("transfers.none") : t("transfers.counts", { active, total: transfers.length })}</span>
          <div className="spacer" />
          <div className="btns">
            <button className="btn btn-secondary" style={{ fontSize: 13 }} disabled={finished === 0} onClick={() => void clearFinished()}>{t("transfers.clearFinished")}</button>
            <button className="btn btn-secondary" style={{ fontSize: 13 }} onClick={() => void pickFile(true)}>{t("transfers.sendFolder")}</button>
            <button className="btn btn-primary" onClick={() => void pickFile()}><Upload size={15} /> {t("transfers.sendFile")}</button>
          </div>
        </div>
      </div>

      {loaded && transfers.length === 0 ? (
        <div className="empty">
          <div className="orb"><Transfers size={40} /></div>
          <h3>{t("transfers.emptyTitle")}</h3>
          <p style={{ maxWidth: 420 }}>{t("transfers.emptyBody")}</p>
          <button className="btn btn-primary" onClick={() => void pickFile()}>{t("transfers.sendFile")}</button>
        </div>
      ) : (
        <div className="list tr-list">
          {transfers.map((rec) => <TransferRow key={rec.id} rec={rec} />)}
        </div>
      )}

      {pendingFiles && <SendFileDialog files={pendingFiles} onClose={() => setPendingFiles(null)} />}
    </div>
  );
}

function TransferRow({ rec }: { rec: TransferRecord }) {
  const t = useT();
  const { kind, dev, incoming, active, busy, hasBar, pct, primary, asked, accept, decline, cancel, resume, open } = useTransferRow(rec, true);
  const reveal = useApp((s) => s.revealTransfer);
  const removeTransfer = useApp((s) => s.removeTransfer);
  const [menu, setMenu] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menu) return;
    const onDoc = (e: MouseEvent) => { if (!menuRef.current?.contains(e.target as Node)) setMenu(false); };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [menu]);

  return (
    <div className="tr-row">
      <div className={`tr-icon${incoming ? " in" : ""}${kind === "failed" ? " failed" : ""}`}><File size={18} /></div>
      <div style={{ minWidth: 0 }}>
        <div className="tr-name" title={rec.originalName}>{rec.safeName}</div>
        <div className="tr-meta">
          <span className={`tr-dir${incoming ? " in" : ""}`}>{incoming ? t("transfers.receiving") : t("transfers.sending")}</span>
          <span>{dev}</span><span>·</span><span>{bytes(rec.size)}</span>
          {isClipImage(rec) && <><span>·</span><span>{t("transfers.clipImage")}</span></>}
        </div>
      </div>
      <div>
        {hasBar && <div className={`tr-bar ${kind}`}><span style={{ width: `${pct}%` }} /></div>}
        <div className={`tr-detail${kind === "failed" ? " failed" : ""}`}>{transferDetail(rec, dev)}</div>
      </div>
      <span className={`tag ${TRANSFER_TAG[kind]}`}>{TRANSFER_LABEL[kind]}</span>
      <div className="tr-actions">
        {asked ? (
          <div className="row-flex" style={{ gap: 6 }}>
            <button className="btn btn-secondary" onClick={decline}>{t("common.decline")}</button>
            <button className="btn btn-primary" onClick={accept}>{t("common.accept")}</button>
          </div>
        ) : primary && <button className="btn btn-secondary" disabled={primary.disabled} onClick={primary.run}>{primary.label}</button>}
        <div className="menu-wrap" ref={menuRef}>
          <button className="icon-btn" title={t("transfers.more")} onClick={() => setMenu((m) => !m)}><More size={15} /></button>
          {menu && (
            <div className="tr-menu">
              {kind === "completed" && <button onClick={() => { setMenu(false); void reveal(rec.id); }}>{t("common.showInFolder")}</button>}
              {kind === "completed" && <button onClick={() => { setMenu(false); open(); }}>{t("common.open")}</button>}
              {(kind === "cancelled" || kind === "failed") && !incoming && <button onClick={() => { setMenu(false); resume(); }}>{t("common.retry")}</button>}
              {active && kind !== "verifying" && <button className="danger" disabled={busy} onClick={() => { setMenu(false); cancel(); }}>{t("transfers.cancel")}</button>}
              {!active && <button onClick={() => { setMenu(false); void removeTransfer(rec.id); }}>{t("transfers.removeFromList")}</button>}
              <div style={{ padding: "6px 12px 4px", fontSize: 11, color: "var(--color-neutral-600)" }}>{t("transfers.attempts", { n: rec.attempts })} · {rec.chunkSize ? t("transfers.perChunk", { size: bytes(rec.chunkSize) }) : ""}</div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
