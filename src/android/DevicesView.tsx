import { useEffect, useRef, useState } from "react";
import { useT } from "../i18n";
import { Back, Plus, Shield } from "../components/Icons";
import { Avatar, Confirm, DeviceBadge, DeviceFlag, Prompt } from "../components/ui";
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
  const t = useT();
  const me = useApp((s) => s.devices.find((d) => d.isSelf));
  const peers = usePeers();
  const now = useApp((s) => s.now);
  const settings = useApp((s) => s.settings);
  const setDeviceFlags = useApp((s) => s.setDeviceFlags);
  const removeDevice = useApp((s) => s.removeDevice);
  const setDeviceAddress = useApp((s) => s.setDeviceAddress);
  const [confirm, setConfirm] = useState<Device | null>(null);
  const [editAddr, setEditAddr] = useState<Device | null>(null);
  const discovered = useDiscovered();
  const requestPairing = useApp((s) => s.requestPairing);
  const [requesting, setRequesting] = useState<string | null>(null);

  const all = me ? [me, ...peers] : peers;

  return (
    <>
      <div className="and-head" style={{ padding: "16px 18px 12px" }}>
        <span className="page-title">{t("nav.devices")}</span>
        <button className="fab" aria-label={t("devices.add")} onClick={onAdd}><Plus size={20} /></button>
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
                    {d.isSelf
                      ? t("devices.thisDevice", { platform: PLATFORM_LABEL[d.platform] ?? d.platform })
                      : `${PLATFORM_LABEL[d.platform] ?? d.platform} · ${d.lastError ? d.lastError : d.state === "connected" ? t("android.devices.seenNow", { addr: d.address ? ` · ${d.address.replace(/:47820$/, "")}` : "" }) : d.state === "connecting" ? t("devices.connecting") : t("devices.seen", { when: relTime(d.lastSeen, now) })}`}
                  </div>
                </div>
                <DeviceBadge device={d} />
              </div>
              {!d.isSelf && (
                <div className="bottom opts">
                  <DeviceFlag label={t("devices.sendAuto")} help={t("devices.sendAutoHelp")} on={d.autoSend}
                    globalOff={settings?.autoSend === false} disabled={d.state === "blocked"} onChange={(v) => setDeviceFlags(d.deviceId, { autoSend: v })} />
                  <DeviceFlag label={t("devices.receiveAuto")} help={t("devices.receiveAutoHelp")} on={d.autoReceive}
                    globalOff={settings?.autoReceive === false} disabled={d.state === "blocked"} onChange={(v) => setDeviceFlags(d.deviceId, { autoReceive: v })} />
                  <div className="row-btns">
                    <button className="btn btn-ghost plain" disabled={d.state === "blocked"} onClick={() => setEditAddr(d)}>{t("android.devices.setAddress")}</button>
                    <button className="btn btn-ghost" onClick={() => setConfirm(d)}>{t("devices.remove")}</button>
                  </div>
                </div>
              )}
            </div>
          ))}
          {peers.length === 0 && (
            <button className="btn btn-primary btn-block" style={{ height: 52, fontWeight: 700, fontSize: 15 }} onClick={onAdd}>{t("android.devices.pairWithPc")}</button>
          )}
          {discovered.length > 0 && <div className="section-title" style={{ marginTop: 14 }}>{t("devices.discovered")}</div>}
          {discovered.map((d) => (
            <div className="and-dev-card" key={d.deviceId}>
              <div className="top">
                <Avatar device={d} />
                <div className="info">
                  <div className="name">{d.name}</div>
                  <div className="detail">{PLATFORM_LABEL[d.platform] ?? d.platform} · {d.address?.replace(/:47820$/, "") ?? "visto agora"}</div>
                </div>
              </div>
              <div className="bottom">
                <span>{t("android.devices.askCode", { name: d.name })}</span>
                <button
                  className="btn btn-secondary"
                  disabled={requesting !== null}
                  onClick={async () => { setRequesting(d.deviceId); await requestPairing(d); setRequesting(null); }}
                >
                  {requesting === d.deviceId ? t("android.home.statusSending") : t("android.devices.request")}
                </button>
              </div>
            </div>
          ))}
        </div>
        <div className="and-note">
          <Shield size={20} style={{ color: "var(--color-accent-2-800)" }} />
          <span>{t("devices.note")}</span>
        </div>
      </div>
      {editAddr && (
        <Prompt
          title={t("devices.addressTitle", { name: editAddr.name })}
          body={t("devices.addressBody")}
          initial={editAddr.address?.replace(/:47820$/, "") ?? ""}
          placeholder="192.168.0.10"
          inputMode="url"
          onCancel={() => setEditAddr(null)}
          onSubmit={async (addr) => { await setDeviceAddress(editAddr.deviceId, addr); setEditAddr(null); }}
        />
      )}
      {confirm && (
        <Confirm
          title={t("devices.removeTitle", { name: confirm.name })}
          body={t("devices.removeBody")}
          confirmLabel={t("devices.remove")}
          danger
          onCancel={() => setConfirm(null)}
          onConfirm={() => { const d = confirm; setConfirm(null); void removeDevice(d.deviceId); }}
        />
      )}
    </>
  );
}

function usePair() {
  const t = useT();
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
      showToast(t("toast.paired", { name: id.name }));
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
  const t = useT();
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
        <button aria-label={t("android.back")} onClick={onBack}><Back size={22} /></button>
        <span className="title">{t("android.scan.title")}</span>
      </div>
      <div className="body">
        <div className={`frame${scanning ? " live" : ""}`}><div className="line" /></div>
        <div className="copy">
          <h3>{busy ? t("pair.pairing") : t("android.scan.aim")}</h3>
          <p>{busy ? t("android.scan.exchanging") : t("android.scan.aimBody")}</p>
        </div>
        {error && <div className="error">{error}</div>}
      </div>
      <div className="foot">
        <button className="btn btn-secondary btn-block" onClick={onManual}>{t("android.scan.manual")}</button>
      </div>
    </div>
  );
}

/** Sem mDNS o PC não aparece: em vez do erro cru (do Rust, nos dois idiomas), aponta para o campo de endereço. */
function notFoundHint(error: string, t: ReturnType<typeof useT>) {
  return error.includes("nenhum dispositivo em modo de pareamento") || error.includes("no device in pairing mode") ? t("android.manual.notFound") : error;
}

function ManualView({ onBack, onScan }: { onBack: () => void; onScan: () => void }) {
  const t = useT();
  const { busy, error, run } = usePair();
  const [code, setCode] = useState("");
  const [addr, setAddr] = useState("");
  const submit = async () => {
    const ok = await run(() => api.pairWithCode(code, undefined, addr.trim() || undefined));
    if (ok) onBack();
  };
  return (
    <div className="scan">
      <div className="head">
        <button aria-label={t("android.back")} onClick={onBack}><Back size={22} /></button>
        <span className="title">{t("android.manual.title")}</span>
      </div>
      <div className="body">
        <div className="copy">
          <h3>{t("android.manual.heading")}</h3>
          <p>{t("android.manual.body")}</p>
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
        <input
          className="input"
          value={addr}
          inputMode="url"
          autoCapitalize="none"
          placeholder="192.168.0.10"
          aria-label={t("android.manual.addrLabel")}
          onChange={(e) => setAddr(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") void submit(); }}
        />
        <p className="hint">{t("android.manual.addrHint")}</p>
        {error && <div className="error">{notFoundHint(error, t)}</div>}
      </div>
      <div className="foot">
        <button className="btn btn-primary btn-block" style={{ height: 52, fontWeight: 700, fontSize: 15 }} disabled={busy || code.replace(/[^a-z0-9]/gi, "").length !== 6} onClick={submit}>
          {busy ? t("pair.pairing") : t("pair.pair")}
        </button>
        <button className="btn btn-secondary btn-block" onClick={onScan}>{t("android.manual.scanQr")}</button>
      </div>
    </div>
  );
}
