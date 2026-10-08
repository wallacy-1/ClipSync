import { useState } from "react";
import { Alert, Plus, Shield, Trash } from "../components/Icons";
import { Avatar, Confirm, DeviceBadge, Toggle } from "../components/ui";
import { PLATFORM_LABEL, mmss, relTime } from "../lib/format";
import type { Device } from "../lib/types";
import { useDiscovered, usePeers, useApp } from "../store/app";
import { api } from "../lib/ipc";

export function DevicesView() {
  const me = useApp((s) => s.me);
  const peers = usePeers();
  const discovered = useDiscovered();
  const pairing = useApp((s) => s.pairing);
  const startPairing = useApp((s) => s.startPairing);
  const cancelPairing = useApp((s) => s.cancelPairing);
  const setDeviceFlags = useApp((s) => s.setDeviceFlags);
  const removeDevice = useApp((s) => s.removeDevice);
  const now = useApp((s) => s.now);
  const [confirmRemove, setConfirmRemove] = useState<Device | null>(null);
  const requestPairing = useApp((s) => s.requestPairing);
  const [requesting, setRequesting] = useState<string | null>(null);

  const failed = peers.filter((d) => d.lastError || (d.queued > 0 && d.state !== "connected"));
  const meDevice = useApp((s) => s.devices.find((d) => d.isSelf));

  return (
    <div className="main">
      <div className="page">
        {failed.map((d) => (
          <div className="banner error" key={d.deviceId}>
            <Alert size={18} />
            <span>
              {d.lastError
                ? `Falha ao enviar para ${d.name} — ${d.lastError}.`
                : `${d.queued} ${d.queued === 1 ? "item aguarda" : "itens aguardam"} ${d.name} voltar à rede.`}
            </span>
            {d.lastError ? (
              <button className="btn btn-secondary" onClick={() => setConfirmRemove(d)}>Reparear</button>
            ) : (
              <button className="btn btn-secondary" onClick={() => api.connectDevice(d.deviceId)}>Tentar agora</button>
            )}
          </div>
        ))}

        <div className="page-head">
          <h4>Seus dispositivos</h4>
          {pairing ? (
            <button className="btn btn-secondary" onClick={cancelPairing}>Cancelar pareamento</button>
          ) : (
            <button className="btn btn-primary" onClick={startPairing}><Plus size={15} /> Adicionar dispositivo</button>
          )}
        </div>

        <div className="dev-list">
          {meDevice && (
            <div className="dev-card">
              <Avatar device={meDevice} />
              <div className="info">
                <div className="name">{me?.name}</div>
                <div className="detail">{PLATFORM_LABEL[meDevice.platform] ?? meDevice.platform} · este dispositivo</div>
              </div>
              <DeviceBadge device={meDevice} />
            </div>
          )}
          {peers.map((d) => (
            <div className="dev-card" key={d.deviceId}>
              <Avatar device={d} />
              <div className="info">
                <div className="name">{d.name}</div>
                <div className="detail">
                  {PLATFORM_LABEL[d.platform] ?? d.platform} ·{" "}
                  {d.lastError
                    ? d.lastError
                    : d.state === "connected"
                      ? `visto agora${d.address ? ` · ${d.address.replace(/:\d+$/, "")}` : ""}`
                      : d.state === "connecting"
                        ? "conectando…"
                        : `visto ${relTime(d.lastSeen, now)}`}
                </div>
              </div>
              <DeviceBadge device={d} />
              <div className="ctl">
                <span>Enviar automático</span>
                <Toggle on={d.autoSend} disabled={d.state === "blocked"} onChange={(v) => setDeviceFlags(d.deviceId, { autoSend: v })} />
              </div>
              <div className="menu">
                <button className="icon-btn" title="Remover e revogar chaves" onClick={() => setConfirmRemove(d)}><Trash size={15} /></button>
              </div>
            </div>
          ))}
        </div>

        {pairing && <PairingCard expiresAt={pairing.expiresAt} code={pairing.code} qrSvg={pairing.qrSvg} now={now} />}

        {discovered.length > 0 && (
          <>
            <div className="section-title" style={{ margin: "24px 0 10px" }}>Na rede, ainda não pareados</div>
            <div className="dev-list">
              {discovered.map((d) => (
                <div className="dev-card" key={d.deviceId}>
                  <Avatar device={d} />
                  <div className="info">
                    <div className="name">{d.name}</div>
                    <div className="detail">{PLATFORM_LABEL[d.platform] ?? d.platform} · {d.address?.replace(/:\d+$/, "") ?? "visto agora"}</div>
                  </div>
                  <DeviceBadge device={d} />
                  <button
                    className="btn btn-secondary"
                    disabled={requesting !== null}
                    title="Pede ao outro dispositivo que mostre um código para digitar aqui"
                    onClick={async () => { setRequesting(d.deviceId); await requestPairing(d); setRequesting(null); }}
                  >
                    {requesting === d.deviceId ? "Enviando pedido…" : "Solicitar pareamento"}
                  </button>
                </div>
              ))}
            </div>
          </>
        )}

        <div className="security-note" style={{ marginTop: 22 }}>
          <Shield size={20} style={{ color: "var(--color-accent-2-800)" }} />
          <span>Pareamento na rede local, cifrado ponta a ponta. Remova um dispositivo e as chaves dele são revogadas.</span>
        </div>
      </div>

      {confirmRemove && (
        <Confirm
          title={confirmRemove.lastError ? `Reparear ${confirmRemove.name}?` : `Remover ${confirmRemove.name}?`}
          body={confirmRemove.lastError
            ? "O vínculo atual será removido e as chaves revogadas. Depois, gere um novo QR em “Adicionar dispositivo” e escaneie pelo outro aparelho."
            : "Este dispositivo deixa de receber e enviar itens. Para voltar, será preciso parear de novo."}
          confirmLabel={confirmRemove.lastError ? "Remover e reparear" : "Remover"}
          danger
          onCancel={() => setConfirmRemove(null)}
          onConfirm={async () => {
            const d = confirmRemove;
            setConfirmRemove(null);
            await removeDevice(d.deviceId);
            if (d.lastError) await startPairing();
          }}
        />
      )}
    </div>
  );
}

export function PairingCard({ expiresAt, code, qrSvg, now }: { expiresAt: number; code: string; qrSvg: string; now: number }) {
  return (
    <div className="pair-card">
      <div className="qr" dangerouslySetInnerHTML={{ __html: qrSvg }} />
      <div className="grow">
        <h4>Pareie pelo celular</h4>
        <p>
          Abra o ClipSync no Android, toque em <b>Dispositivos → Adicionar</b> e escaneie este código. O par é feito na rede local, sem servidor.
        </p>
        <div className="row-flex" style={{ gap: 12 }}>
          <span className="pair-code">{code}</span>
          <span className="pair-expires">expira em {mmss(expiresAt - now)}</span>
        </div>
      </div>
    </div>
  );
}
