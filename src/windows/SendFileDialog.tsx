import { useEffect, useState } from "react";
import { File, MonitorOff, Shield } from "../components/Icons";
import { Avatar } from "../components/ui";
import { PLATFORM_LABEL, bytes } from "../lib/format";
import { api } from "../lib/ipc";
import type { Device } from "../lib/types";
import { usePeers, useApp } from "../store/app";

export interface Target {
  device: Device;
  selectable: boolean;
  /** Motivo de não estar disponível (esmaecido). */
  cap: string | null;
  detail: string;
}

/** Classifica os pares para envio de arquivo. Consulta `file-v1` de cada conectado. */
export function useFileTargets(): { targets: Target[]; loading: boolean } {
  const peers = usePeers();
  const [supports, setSupports] = useState<Record<string, boolean>>({});
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let alive = true;
    const connected = peers.filter((d) => d.state === "connected");
    Promise.all(connected.map((d) => api.transferPeerSupports(d.deviceId).catch(() => false))).then((res) => {
      if (!alive) return;
      const map: Record<string, boolean> = {};
      connected.forEach((d, i) => { map[d.deviceId] = res[i]; });
      setSupports(map);
      setLoading(false);
    });
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [peers.map((d) => `${d.deviceId}:${d.state}`).join(",")]);

  const targets = peers.map<Target>((d) => {
    const os = PLATFORM_LABEL[d.platform] ?? d.platform;
    if (d.state === "blocked") return { device: d, selectable: false, cap: "Bloqueado", detail: `${os} · bloqueado` };
    if (d.state !== "connected") return { device: d, selectable: false, cap: "Offline", detail: `${os} · offline` };
    if (supports[d.deviceId] === false) return { device: d, selectable: false, cap: "Sem suporte", detail: `${os} · versão antiga do ClipSync` };
    return { device: d, selectable: true, cap: null, detail: `${os} · conectado` };
  });
  return { targets, loading };
}

export function SendFileDialog({ file, onClose }: { file: { path: string; name: string; size: number }; onClose: () => void }) {
  const { targets, loading } = useFileTargets();
  const sendFile = useApp((s) => s.sendFile);
  const setView = useApp((s) => s.setView);
  const [selected, setSelected] = useState<string | null>(null);
  const [sending, setSending] = useState(false);

  const available = targets.filter((t) => t.selectable);
  useEffect(() => {
    if (selected === null && available.length > 0) setSelected(available[0].device.deviceId);
  }, [available, selected]);

  const chosen = targets.find((t) => t.device.deviceId === selected);
  const none = !loading && available.length === 0;

  return (
    <div className="dialog-backdrop" onClick={onClose}>
      <div className="dialog sendfile" onClick={(e) => e.stopPropagation()}>
        <div className="head">
          <div className="tr-icon"><File size={20} /></div>
          <div style={{ minWidth: 0 }}>
            <h3>Enviar arquivo</h3>
            <div className="file" title={file.path}>{file.name}{file.size ? ` · ${bytes(file.size)}` : ""}</div>
          </div>
        </div>

        {none ? (
          <div className="none">
            <div className="orb"><MonitorOff size={32} /></div>
            <h3>Nenhum dispositivo disponível</h3>
            <p>Seus dispositivos pareados estão offline ou não têm suporte a transferência de arquivos. O arquivo não foi enfileirado — tente novamente quando algum deles voltar.</p>
            <div className="btns">
              <button className="btn btn-secondary" onClick={() => { onClose(); setView("devices"); }}>Ver dispositivos</button>
              <button className="btn btn-primary" onClick={onClose}>Tentar novamente</button>
            </div>
          </div>
        ) : (
          <div>
            <div className="section-title">Destino</div>
            <div className="targets">
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
              {loading && targets.length === 0 && <div className="muted" style={{ fontSize: 13.5 }}>Procurando dispositivos…</div>}
            </div>
            <div className="note"><Shield size={15} /> Enviado direto pela rede local, cifrado ponta a ponta.</div>
            <div className="actions">
              <button className="btn btn-secondary" onClick={onClose}>Cancelar</button>
              <button
                className="btn btn-primary"
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
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
