import { useEffect, useRef, useState } from "react";
import { Back, Plus, Shield } from "../components/Icons";
import { Avatar, Confirm, DeviceBadge, Toggle } from "../components/ui";
import { PLATFORM_LABEL, relTime } from "../lib/format";
import { api, cancelScan, errorText, inTauri, scanQr } from "../lib/ipc";
import type { Device } from "../lib/types";
import { useDiscovered, usePeers, useApp } from "../store/app";

type Mode = "list" | "scan" | "manual";

export function DevicesView({ autoScan, onScanHandled }: { autoScan: boolean; onScanHandled: () => void }) {
  const [mode, setMode] = useState<Mode>(autoScan ? "scan" : "list");
  useEffect(() => { if (autoScan) { setMode("scan"); onScanHandled(); } }, [autoScan, onScanHandled]);

  if (mode === "scan") return <ScanView onBack={() => setMode("list")} onManual={() => setMode("manual")} />;
  if (mode === "manual") return <ManualView onBack={() => setMode("list")} onScan={() => setMode("scan")} />;
  return <ListView onAdd={() => setMode("scan")} />;
}

function ListView({ onAdd }: { onAdd: () => void }) {
  const me = useApp((s) => s.devices.find((d) => d.isSelf));
  const peers = usePeers();
  const now = useApp((s) => s.now);
  const setDeviceFlags = useApp((s) => s.setDeviceFlags);
  const removeDevice = useApp((s) => s.removeDevice);
  const [confirm, setConfirm] = useState<Device | null>(null);
  const discovered = useDiscovered();
  const requestPairing = useApp((s) => s.requestPairing);
  const [requesting, setRequesting] = useState<string | null>(null);

  const all = me ? [me, ...peers] : peers;

  return (
    <>
      <div className="and-head" style={{ padding: "16px 18px 12px" }}>
        <span className="page-title">Dispositivos</span>
        <button className="fab" aria-label="Adicionar dispositivo" onClick={onAdd}><Plus size={20} /></button>
      </div>
      <div className="and-scroll">
        <div style={{ padding: "0 18px", display: "flex", flexDirection: "column", gap: 10 }}>
          {all.map((d) => (
            <div className="and-dev-card" key={d.deviceId}>
              <div className="top">
                <Avatar device={d} />
                <div className="info">
                  <div className="name">{d.name}</div>
                  <div className="detail">
                    {PLATFORM_LABEL[d.platform] ?? d.platform} ·{" "}
                    {d.isSelf ? "este dispositivo" : d.lastError ? d.lastError : d.state === "connected" ? `visto agora${d.address ? ` · ${d.address.replace(/:\d+$/, "")}` : ""}` : d.state === "connecting" ? "conectando…" : `visto ${relTime(d.lastSeen, now)}`}
                  </div>
                </div>
                <DeviceBadge device={d} />
              </div>
              {!d.isSelf && (
                <div className="bottom">
                  <span>Receber automático</span>
                  <Toggle on={d.autoReceive} disabled={d.state === "blocked"} onChange={(v) => setDeviceFlags(d.deviceId, { autoReceive: v })} />
                  <button className="btn btn-ghost" onClick={() => setConfirm(d)}>Remover</button>
                </div>
              )}
            </div>
          ))}
          {peers.length === 0 && (
            <button className="btn btn-primary btn-block" style={{ height: 52, fontWeight: 700, fontSize: 15 }} onClick={onAdd}>Parear com o PC</button>
          )}
          {discovered.length > 0 && <div className="section-title" style={{ marginTop: 14 }}>Na rede, ainda não pareados</div>}
          {discovered.map((d) => (
            <div className="and-dev-card" key={d.deviceId}>
              <div className="top">
                <Avatar device={d} />
                <div className="info">
                  <div className="name">{d.name}</div>
                  <div className="detail">{PLATFORM_LABEL[d.platform] ?? d.platform} · {d.address?.replace(/:\d+$/, "") ?? "visto agora"}</div>
                </div>
              </div>
              <div className="bottom">
                <span>Pede que {d.name} mostre um código</span>
                <button
                  className="btn btn-secondary"
                  disabled={requesting !== null}
                  onClick={async () => { setRequesting(d.deviceId); await requestPairing(d); setRequesting(null); }}
                >
                  {requesting === d.deviceId ? "Enviando…" : "Solicitar"}
                </button>
              </div>
            </div>
          ))}
        </div>
        <div className="and-note">
          <Shield size={20} style={{ color: "var(--color-accent-2-800)" }} />
          <span>Pareamento na rede local, cifrado ponta a ponta. Remova um dispositivo e as chaves dele são revogadas.</span>
        </div>
      </div>
      {confirm && (
        <Confirm
          title={`Remover ${confirm.name}?`}
          body="Este dispositivo deixa de receber e enviar itens. Para voltar, será preciso parear de novo."
          confirmLabel="Remover"
          danger
          onCancel={() => setConfirm(null)}
          onConfirm={() => { const d = confirm; setConfirm(null); void removeDevice(d.deviceId); }}
        />
      )}
    </>
  );
}

function usePair() {
  const showToast = useApp((s) => s.showToast);
  const refreshDevices = useApp((s) => s.refreshDevices);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = async (fn: () => Promise<{ name: string }>): Promise<boolean> => {
    setBusy(true);
    setError(null);
    try {
      const id = await fn();
      await refreshDevices();
      showToast(`${id.name} pareado`);
      return true;
    } catch (e) {
      setError(errorText(e));
      return false;
    } finally {
      setBusy(false);
    }
  };
  return { busy, error, run, setError };
}

function ScanView({ onBack, onManual }: { onBack: () => void; onManual: () => void }) {
  const { busy, error, run, setError } = usePair();
  const [scanning, setScanning] = useState(false);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    let cancelled = false;
    (async () => {
      setScanning(true);
      if (inTauri) { document.documentElement.style.background = "transparent"; document.body.style.background = "transparent"; }
      try {
        const raw = await scanQr();
        if (cancelled || !raw) return;
        setScanning(false);
        const ok = await run(() => api.pairWithPayload(raw));
        if (ok && alive.current) onBack();
      } catch (e) {
        if (!cancelled) setError(errorText(e));
      } finally {
        if (!cancelled) setScanning(false);
      }
    })();
    return () => {
      cancelled = true; alive.current = false; void cancelScan();
      document.documentElement.style.background = ""; document.body.style.background = "";
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Com a câmera ativa o plugin desenha o vídeo por trás da WebView; o fundo fica transparente.
  return (
    <div className={`scan${scanning && inTauri ? " transparent" : ""}`}>
      <div className="head">
        <button aria-label="Voltar" onClick={onBack}><Back size={22} /></button>
        <span className="title">Escanear código</span>
      </div>
      <div className="body">
        <div className={`frame${scanning ? " live" : ""}`}><div className="line" /></div>
        <div className="copy">
          <h3>{busy ? "Pareando…" : "Aponte para o PC"}</h3>
          <p>{busy ? "Trocando chaves pela rede local." : "No Windows: Dispositivos → Adicionar dispositivo. O código aparece na janela."}</p>
        </div>
        {error && <div className="error">{error}</div>}
      </div>
      <div className="foot">
        <button className="btn btn-secondary btn-block" onClick={onManual}>Digitar código manualmente</button>
      </div>
    </div>
  );
}

function ManualView({ onBack, onScan }: { onBack: () => void; onScan: () => void }) {
  const { busy, error, run } = usePair();
  const [code, setCode] = useState("");
  const submit = async () => {
    const ok = await run(() => api.pairWithCode(code));
    if (ok) onBack();
  };
  return (
    <div className="scan">
      <div className="head">
        <button aria-label="Voltar" onClick={onBack}><Back size={22} /></button>
        <span className="title">Digitar código</span>
      </div>
      <div className="body">
        <div className="copy">
          <h3>Código do PC</h3>
          <p>Os 6 caracteres mostrados ao lado do QR na janela do Windows. O PC precisa estar na mesma rede.</p>
        </div>
        <input
          className="input"
          value={code}
          autoFocus
          maxLength={7}
          placeholder="4K7-Q2M"
          autoCapitalize="characters"
          onChange={(e) => setCode(e.target.value.toUpperCase())}
          onKeyDown={(e) => { if (e.key === "Enter") void submit(); }}
        />
        {error && <div className="error">{error}</div>}
      </div>
      <div className="foot">
        <button className="btn btn-primary btn-block" style={{ height: 52, fontWeight: 700, fontSize: 15 }} disabled={busy || code.replace(/[^a-z0-9]/gi, "").length !== 6} onClick={submit}>
          {busy ? "Pareando…" : "Parear"}
        </button>
        <button className="btn btn-secondary btn-block" onClick={onScan}>Escanear QR</button>
      </div>
    </div>
  );
}
