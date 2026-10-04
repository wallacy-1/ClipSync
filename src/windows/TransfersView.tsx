import { useEffect, useRef, useState } from "react";
import { File, More, Transfers, Upload } from "../components/Icons";
import { useTransferRow } from "../components/transfer";
import { bytes, isClipImage, isTransferActive, pluralize, transferDetail, TRANSFER_LABEL, TRANSFER_TAG } from "../lib/format";
import { onFileDrop } from "../lib/ipc";
import type { TransferRecord } from "../lib/types";
import { useApp } from "../store/app";
import { SendFileDialog } from "./SendFileDialog";

export function TransfersView() {
  const transfers = useApp((s) => s.transfers);
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
      {dragging && <div className="drop-hint">Solte para enviar</div>}
      <div className="toolbar">
        <div className="page-head" style={{ margin: 0 }}>
          <h4>Transferências</h4>
          <span className="count">{transfers.length === 0 ? "nenhuma" : `${active} ativas · ${transfers.length} na lista`}</span>
          <div className="spacer" />
          <div className="btns">
            <button className="btn btn-secondary" style={{ fontSize: 13 }} disabled={finished === 0} onClick={() => void clearFinished()}>Limpar concluídas</button>
            <button className="btn btn-secondary" style={{ fontSize: 13 }} onClick={() => void pickFile(true)}>Enviar pasta</button>
            <button className="btn btn-primary" onClick={() => void pickFile()}><Upload size={15} /> Enviar arquivo</button>
          </div>
        </div>
      </div>

      {loaded && transfers.length === 0 ? (
        <div className="empty">
          <div className="orb"><Transfers size={40} /></div>
          <h3>Nenhuma transferência</h3>
          <p style={{ maxWidth: 420 }}>Envie arquivos ou pastas pelos botões acima ou arraste-os para esta janela (pastas vão como .zip). Arquivos recebidos aparecem aqui automaticamente.</p>
          <button className="btn btn-primary" onClick={() => void pickFile()}>Enviar arquivo</button>
        </div>
      ) : (
        <div className="list tr-list">
          {transfers.map((t) => <TransferRow key={t.id} t={t} />)}
        </div>
      )}

      {pendingFiles && <SendFileDialog files={pendingFiles} onClose={() => setPendingFiles(null)} />}
    </div>
  );
}

function TransferRow({ t }: { t: TransferRecord }) {
  const { kind, dev, incoming, active, busy, hasBar, pct, primary, asked, accept, decline, cancel, resume, open } = useTransferRow(t, true);
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
        <div className="tr-name" title={t.originalName}>{t.safeName}</div>
        <div className="tr-meta">
          <span className={`tr-dir${incoming ? " in" : ""}`}>{incoming ? "Recebendo" : "Enviando"}</span>
          <span>{dev}</span><span>·</span><span>{bytes(t.size)}</span>
          {isClipImage(t) && <><span>·</span><span>imagem copiada</span></>}
        </div>
      </div>
      <div>
        {hasBar && <div className={`tr-bar ${kind}`}><span style={{ width: `${pct}%` }} /></div>}
        <div className={`tr-detail${kind === "failed" ? " failed" : ""}`}>{transferDetail(t, dev)}</div>
      </div>
      <span className={`tag ${TRANSFER_TAG[kind]}`}>{TRANSFER_LABEL[kind]}</span>
      <div className="tr-actions">
        {asked ? (
          <div className="row-flex" style={{ gap: 6 }}>
            <button className="btn btn-secondary" onClick={decline}>Recusar</button>
            <button className="btn btn-primary" onClick={accept}>Aceitar</button>
          </div>
        ) : primary && <button className="btn btn-secondary" disabled={primary.disabled} onClick={primary.run}>{primary.label}</button>}
        <div className="menu-wrap" ref={menuRef}>
          <button className="icon-btn" title="Mais" onClick={() => setMenu((m) => !m)}><More size={15} /></button>
          {menu && (
            <div className="tr-menu">
              {kind === "completed" && <button onClick={() => { setMenu(false); void reveal(t.id); }}>Mostrar na pasta</button>}
              {kind === "completed" && <button onClick={() => { setMenu(false); open(); }}>Abrir</button>}
              {(kind === "cancelled" || kind === "failed") && !incoming && <button onClick={() => { setMenu(false); resume(); }}>Tentar de novo</button>}
              {active && kind !== "verifying" && <button className="danger" disabled={busy} onClick={() => { setMenu(false); cancel(); }}>Cancelar</button>}
              {!active && <button onClick={() => { setMenu(false); void removeTransfer(t.id); }}>Remover da lista</button>}
              <div style={{ padding: "6px 12px 4px", fontSize: 11, color: "var(--color-neutral-600)" }}>{pluralize(t.attempts, "tentativa", "tentativas")} · {t.chunkSize ? `${bytes(t.chunkSize)} por bloco` : ""}</div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
