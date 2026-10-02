import { useEffect, useRef, useState } from "react";
import { File, More, Transfers, Upload } from "../components/Icons";
import { bytes, deviceName, isClipImage, isTransferActive, percent, pluralize, transferDetail, TRANSFER_LABEL, TRANSFER_TAG } from "../lib/format";
import { onFileDrop } from "../lib/ipc";
import type { TransferRecord } from "../lib/types";
import { useApp } from "../store/app";
import { SendFileDialog } from "./SendFileDialog";

export function TransfersView() {
  const transfers = useApp((s) => s.transfers);
  const loaded = useApp((s) => s.transfersLoaded);
  const refresh = useApp((s) => s.refreshTransfers);
  const pickFile = useApp((s) => s.pickFile);
  const setPendingFile = useApp((s) => s.setPendingFile);
  const pendingFile = useApp((s) => s.pendingFile);
  const clearFinished = useApp((s) => s.clearFinishedTransfers);
  const [dragging, setDragging] = useState(false);

  useEffect(() => {
    if (!loaded) void refresh();
  }, [loaded, refresh]);

  // Arquivo solto sobre a janela: mesmo fluxo do botão, sem seletor.
  useEffect(() => {
    let off: (() => void) | undefined;
    void onFileDrop((paths) => {
      const path = paths[0];
      const name = path.split(/[\\/]/).pop() ?? path;
      setPendingFile({ path, name, size: 0 });
      setDragging(false);
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
  }, [setPendingFile]);

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
            <button className="btn btn-secondary" style={{ fontSize: 13 }} disabled={finished === 0} onClick={clearFinished}>Limpar concluídas</button>
            <button className="btn btn-primary" onClick={pickFile}><Upload size={15} /> Enviar arquivo</button>
          </div>
        </div>
      </div>

      {loaded && transfers.length === 0 ? (
        <div className="empty">
          <div className="orb"><Transfers size={40} /></div>
          <h3>Nenhuma transferência</h3>
          <p style={{ maxWidth: 420 }}>Envie um arquivo pelo botão acima ou arraste-o para esta janela. Arquivos recebidos aparecem aqui automaticamente.</p>
          <button className="btn btn-primary" onClick={pickFile}>Enviar arquivo</button>
        </div>
      ) : (
        <div className="list tr-list">
          {transfers.map((t) => <TransferRow key={t.id} t={t} />)}
        </div>
      )}

      {pendingFile && <SendFileDialog file={pendingFile} onClose={() => setPendingFile(null)} />}
    </div>
  );
}

function TransferRow({ t }: { t: TransferRecord }) {
  const devices = useApp((s) => s.devices);
  const selfId = useApp((s) => s.me?.deviceId ?? "");
  const busy = useApp((s) => !!s.transferBusy[t.id]);
  const asked = useApp((s) => s.asks.includes(t.id));
  const accept = useApp((s) => s.acceptTransfer);
  const decline = useApp((s) => s.declineTransfer);
  const cancel = useApp((s) => s.cancelTransfer);
  const resume = useApp((s) => s.resumeTransfer);
  const open = useApp((s) => s.openTransfer);
  const reveal = useApp((s) => s.revealTransfer);
  const [menu, setMenu] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menu) return;
    const onDoc = (e: MouseEvent) => { if (!menuRef.current?.contains(e.target as Node)) setMenu(false); };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [menu]);

  const kind = t.state.kind;
  const dev = deviceName(devices, t.deviceId, selfId);
  const incoming = t.direction === "in";
  const hasBar = ["transferring", "verifying", "waiting", "cancelled"].includes(kind);
  const pct = kind === "verifying" ? 100 : percent(t);
  const primary = isTransferActive(t)
    ? (kind === "verifying" ? null : { label: busy ? "Cancelando…" : "Cancelar", run: () => cancel(t.id), disabled: busy })
    : kind === "failed" && !incoming
      ? { label: "Tentar de novo", run: () => resume(t.id), disabled: false }
      : kind === "completed"
        ? { label: "Abrir", run: () => open(t.id), disabled: false }
        : null;

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
            <button className="btn btn-secondary" onClick={() => void decline(t.id)}>Recusar</button>
            <button className="btn btn-primary" onClick={() => void accept(t.id)}>Aceitar</button>
          </div>
        ) : primary && <button className="btn btn-secondary" disabled={primary.disabled} onClick={primary.run}>{primary.label}</button>}
        <div className="menu-wrap" ref={menuRef}>
          <button className="icon-btn" title="Mais" onClick={() => setMenu((m) => !m)}><More size={15} /></button>
          {menu && (
            <div className="tr-menu">
              {kind === "completed" && <button onClick={() => { setMenu(false); void reveal(t.id); }}>Mostrar na pasta</button>}
              {kind === "completed" && <button onClick={() => { setMenu(false); void open(t.id); }}>Abrir</button>}
              {(kind === "cancelled" || kind === "failed") && !incoming && <button onClick={() => { setMenu(false); void resume(t.id); }}>Tentar de novo</button>}
              {isTransferActive(t) && kind !== "verifying" && <button className="danger" disabled={busy} onClick={() => { setMenu(false); void cancel(t.id); }}>Cancelar</button>}
              {!isTransferActive(t) && <button onClick={() => { setMenu(false); useApp.setState((s) => ({ transfers: s.transfers.filter((x) => x.id !== t.id) })); }}>Remover da lista</button>}
              <div style={{ padding: "6px 12px 4px", fontSize: 11, color: "var(--color-neutral-600)" }}>{pluralize(t.attempts, "tentativa", "tentativas")} · {t.chunkSize ? `${bytes(t.chunkSize)} por bloco` : ""}</div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
