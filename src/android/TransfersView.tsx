import { useEffect, useState } from "react";
import { File, Shield, Transfers, Upload } from "../components/Icons";
import { Avatar } from "../components/ui";
import { bytes, deviceName, isTransferActive, percent, transferDetail, TRANSFER_LABEL, TRANSFER_TAG } from "../lib/format";
import type { TransferRecord } from "../lib/types";
import { useApp } from "../store/app";
import { useFileTargets } from "../windows/SendFileDialog";

export function TransfersView() {
  const transfers = useApp((s) => s.transfers);
  const loaded = useApp((s) => s.transfersLoaded);
  const refresh = useApp((s) => s.refreshTransfers);
  const pickFile = useApp((s) => s.pickFile);
  const pendingFile = useApp((s) => s.pendingFile);
  const setPendingFile = useApp((s) => s.setPendingFile);

  useEffect(() => {
    if (!loaded) void refresh();
  }, [loaded, refresh]);

  const active = transfers.filter(isTransferActive).length;
  const failed = transfers.filter((t) => t.state.kind === "failed").length;
  const waiting = transfers.filter((t) => t.state.kind === "waiting").length;
  const pill = failed > 0
    ? { cls: "tag-failed", label: `${failed} ${failed === 1 ? "falha" : "falhas"}` }
    : waiting > 0 && active === waiting
      ? { cls: "tag-sending", label: "Reconectando" }
      : active > 0
        ? { cls: "tag-synced", label: `${active} ${active === 1 ? "ativa" : "ativas"}` }
        : { cls: "tag-synced", label: "Sem atividade" };

  return (
    <>
      <div className="and-head" style={{ padding: "16px 18px 12px", gap: 12 }}>
        <span className="page-title">Transferências</span>
        <span className={`tag pill ${pill.cls}`}>{pill.label}</span>
      </div>

      <div className="and-scroll">
        {loaded && transfers.length === 0 ? (
          <div className="and-empty">
            <div className="orb"><Transfers size={36} /></div>
            <h3>Nenhuma transferência</h3>
            <p style={{ lineHeight: 1.5 }}>Toque em Enviar arquivo para escolher um arquivo do dispositivo.</p>
          </div>
        ) : (
          <div className="and-tr-list">
            {transfers.map((t) => <TransferCard key={t.id} t={t} />)}
          </div>
        )}
      </div>

      <div className="and-tr-cta">
        <button className="btn btn-primary btn-block" onClick={pickFile}><Upload size={18} /> Enviar arquivo</button>
      </div>

      {pendingFile && <SendFileSheet file={pendingFile} onClose={() => setPendingFile(null)} />}
    </>
  );
}

function TransferCard({ t }: { t: TransferRecord }) {
  const devices = useApp((s) => s.devices);
  const selfId = useApp((s) => s.me?.deviceId ?? "");
  const busy = useApp((s) => !!s.transferBusy[t.id]);
  const cancel = useApp((s) => s.cancelTransfer);
  const resume = useApp((s) => s.resumeTransfer);
  const open = useApp((s) => s.openTransfer);

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
    <div className="and-tr-card">
      <div className="top">
        <div className={`tr-icon${incoming ? " in" : ""}${kind === "failed" ? " failed" : ""}`}><File size={18} /></div>
        <div className="body">
          <div className="tr-name">{t.safeName}</div>
          <div className="tr-meta">
            <span className={`tr-dir${incoming ? " in" : ""}`}>{incoming ? "Recebendo" : "Enviando"}</span>
            <span>{dev}</span>
          </div>
        </div>
        <span className={`tag ${TRANSFER_TAG[kind]}`}>{TRANSFER_LABEL[kind]}</span>
      </div>
      {hasBar && <div className={`tr-bar ${kind}`}><span style={{ width: `${pct}%` }} /></div>}
      <div className="foot">
        <span className={`tr-detail${kind === "failed" ? " failed" : ""}`}>{transferDetail(t, dev)}</span>
        {primary && <button className="btn btn-secondary" disabled={primary.disabled} onClick={primary.run}>{primary.label}</button>}
      </div>
    </div>
  );
}

function SendFileSheet({ file, onClose }: { file: { path: string; name: string; size: number }; onClose: () => void }) {
  const { targets, loading } = useFileTargets();
  const sendFile = useApp((s) => s.sendFile);
  const [selected, setSelected] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const available = targets.filter((t) => t.selectable);
  useEffect(() => {
    if (selected === null && available.length > 0) setSelected(available[0].device.deviceId);
  }, [available, selected]);
  const chosen = targets.find((t) => t.device.deviceId === selected);

  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" onClick={(e) => e.stopPropagation()}>
        <div className="handle" />
        <h3>Enviar arquivo</h3>
        <div className="sub">Escolha o dispositivo que recebe este arquivo.</div>
        <div className="preview">{file.name}{file.size ? ` · ${bytes(file.size)}` : ""}</div>
        <div className="targets">
          {!loading && available.length === 0 && (
            <div className="muted" style={{ fontSize: 13.5 }}>Nenhum dispositivo disponível: os pareados estão offline ou não aceitam arquivos.</div>
          )}
          {targets.map((t) => (
            <button
              key={t.device.deviceId}
              className={`target-row2${selected === t.device.deviceId ? " selected" : ""}${!t.selectable ? " blocked" : ""}`}
              disabled={!t.selectable}
              onClick={() => setSelected(t.device.deviceId)}
            >
              <Avatar device={t.device} />
              <div className="info">
                <div className="name">{t.device.name}</div>
                <div className="detail">{t.detail}</div>
              </div>
              {t.cap ? <span className="tag tag-queued">{t.cap}</span> : <span className={`radio${selected === t.device.deviceId ? " on" : ""}`} />}
            </button>
          ))}
        </div>
        <button
          className="btn btn-primary btn-block cta"
          disabled={!chosen?.selectable || sending}
          onClick={async () => {
            if (!chosen) return;
            setSending(true);
            await sendFile(chosen.device.deviceId, file.path);
            setSending(false);
            onClose();
          }}
        >
          {chosen ? `Enviar para ${chosen.device.name}` : "Enviar"}
        </button>
        <div className="foot"><Shield size={15} /> Enviado direto pela rede local, cifrado ponta a ponta.</div>
      </div>
    </div>
  );
}
