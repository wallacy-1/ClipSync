import { useState } from "react";
import { Alert, Globe, Plus, Shield, Trash } from "../components/Icons";
import { Avatar, Confirm, DeviceBadge, DeviceFlag, Prompt, rich } from "../components/ui";
import { useT } from "../i18n";
import { PLATFORM_LABEL, mmss, relTime } from "../lib/format";
import type { Device } from "../lib/types";
import { useDiscovered, usePeers, useApp } from "../store/app";
import { api } from "../lib/ipc";

export function DevicesView() {
  const t = useT();
  const me = useApp((s) => s.me);
  const peers = usePeers();
  const discovered = useDiscovered();
  const pairing = useApp((s) => s.pairing);
  const startPairing = useApp((s) => s.startPairing);
  const cancelPairing = useApp((s) => s.cancelPairing);
  const setDeviceFlags = useApp((s) => s.setDeviceFlags);
  const removeDevice = useApp((s) => s.removeDevice);
  const setDeviceAddress = useApp((s) => s.setDeviceAddress);
  const now = useApp((s) => s.now);
  const settings = useApp((s) => s.settings);
  const [confirmRemove, setConfirmRemove] = useState<Device | null>(null);
  const [editAddr, setEditAddr] = useState<Device | null>(null);
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
                ? t("devices.sendFailed", { name: d.name, error: d.lastError })
                : t("devices.waiting", { n: d.queued, name: d.name })}
            </span>
            {d.lastError ? (
              <button className="btn btn-secondary" onClick={() => setConfirmRemove(d)}>{t("devices.repair")}</button>
            ) : (
              <button className="btn btn-secondary" onClick={() => api.connectDevice(d.deviceId)}>{t("common.retryNow")}</button>
            )}
          </div>
        ))}

        <div className="page-head">
          <h4>{t("devices.yourDevices")}</h4>
          {pairing ? (
            <button className="btn btn-secondary" onClick={cancelPairing}>{t("devices.cancelPairing")}</button>
          ) : (
            <button className="btn btn-primary" onClick={startPairing}><Plus size={15} /> {t("devices.add")}</button>
          )}
        </div>

        <div className="dev-list">
          {meDevice && (
            <div className="dev-card">
              <Avatar device={meDevice} />
              <div className="info">
                <div className="name">{me?.name}</div>
                <div className="detail">{t("devices.thisDevice", { platform: PLATFORM_LABEL[meDevice.platform] ?? meDevice.platform })}</div>
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
                      ? `${t("devices.seenNow")}${d.address ? ` · ${d.address.replace(/:47820$/, "")}` : ""}`
                      : d.state === "connecting"
                        ? t("devices.connecting")
                        : t("devices.seen", { when: relTime(d.lastSeen, now) })}
                </div>
              </div>
              <DeviceBadge device={d} />
              <div className="ctl">
                <DeviceFlag label={t("devices.sendAuto")} help={t("devices.sendAutoHelp")} on={d.autoSend}
                  globalOff={settings?.autoSend === false} disabled={d.state === "blocked"} onChange={(v) => setDeviceFlags(d.deviceId, { autoSend: v })} />
                <DeviceFlag label={t("devices.receiveAuto")} help={t("devices.receiveAutoHelp")} on={d.autoReceive}
                  globalOff={settings?.autoReceive === false} disabled={d.state === "blocked"} onChange={(v) => setDeviceFlags(d.deviceId, { autoReceive: v })} />
              </div>
              <div className="menu">
                <button className="icon-btn" title={t("devices.setAddress")} disabled={d.state === "blocked"} onClick={() => setEditAddr(d)}><Globe size={15} /></button>
                <button className="icon-btn" title={t("devices.removeRevoke")} onClick={() => setConfirmRemove(d)}><Trash size={15} /></button>
              </div>
            </div>
          ))}
        </div>

        {pairing && <PairingCard expiresAt={pairing.expiresAt} code={pairing.code} qrSvg={pairing.qrSvg} addrs={pairing.addrs} now={now} />}

        {discovered.length > 0 && (
          <>
            <div className="section-title" style={{ margin: "24px 0 10px" }}>{t("devices.discovered")}</div>
            <div className="dev-list">
              {discovered.map((d) => (
                <div className="dev-card" key={d.deviceId}>
                  <Avatar device={d} />
                  <div className="info">
                    <div className="name">{d.name}</div>
                    <div className="detail">{PLATFORM_LABEL[d.platform] ?? d.platform} · {d.address?.replace(/:47820$/, "") ?? t("devices.seenNow")}</div>
                  </div>
                  <DeviceBadge device={d} />
                  <button
                    className="btn btn-secondary"
                    disabled={requesting !== null}
                    title={t("devices.requestHint")}
                    onClick={async () => { setRequesting(d.deviceId); await requestPairing(d); setRequesting(null); }}
                  >
                    {requesting === d.deviceId ? t("devices.requesting") : t("devices.request")}
                  </button>
                </div>
              ))}
            </div>
          </>
        )}

        <div className="security-note" style={{ marginTop: 22 }}>
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

      {confirmRemove && (
        <Confirm
          title={t(confirmRemove.lastError ? "devices.repairTitle" : "devices.removeTitle", { name: confirmRemove.name })}
          body={t(confirmRemove.lastError ? "devices.repairBody" : "devices.removeBody")}
          confirmLabel={t(confirmRemove.lastError ? "devices.removeAndRepair" : "devices.remove")}
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

export function PairingCard({ expiresAt, code, qrSvg, addrs, now }: { expiresAt: number; code: string; qrSvg: string; addrs: string[]; now: number }) {
  const t = useT();
  return (
    <div className="pair-card">
      <div className="qr" dangerouslySetInnerHTML={{ __html: qrSvg }} />
      <div className="grow">
        <h4>{t("devices.pairTitle")}</h4>
        <p>{rich(t("devices.pairBody"))}</p>
        {addrs.length > 0 && <div className="pair-addr">{rich(t("devices.pairAddress", { addrs: `**${addrs.join(" · ")}**` }))}</div>}
        <div className="row-flex" style={{ gap: 12 }}>
          <span className="pair-code">{code}</span>
          <span className="pair-expires">{t("devices.expiresIn", { time: mmss(expiresAt - now) })}</span>
        </div>
      </div>
    </div>
  );
}
