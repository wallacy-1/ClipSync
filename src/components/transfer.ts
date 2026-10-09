// Lógica de transferência compartilhada pelas telas do Windows e do Android.
import { useEffect, useState } from "react";
import { t } from "../i18n";
import { PLATFORM_LABEL, deviceName, isTransferActive, percent } from "../lib/format";
import { api } from "../lib/ipc";
import type { Device, TransferRecord } from "../lib/types";
import { usePeers, useApp } from "../store/app";

export interface Target {
  device: Device;
  selectable: boolean;
  /** Motivo de não estar disponível (esmaecido). */
  cap: "blocked" | "offline" | "unsupported" | null;
  capLabel: string | null;
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
    if (d.state === "blocked") return { device: d, selectable: false, cap: "blocked", capLabel: t("sendFile.capBlocked"), detail: t("sendFile.detailBlocked", { os }) };
    if (d.state !== "connected") return { device: d, selectable: false, cap: "offline", capLabel: t("sendFile.capOffline"), detail: t("sendFile.detailOffline", { os }) };
    if (supports[d.deviceId] === false) return { device: d, selectable: false, cap: "unsupported", capLabel: t("sendFile.capUnsupported"), detail: t("sendFile.detailOld", { os }) };
    return { device: d, selectable: true, cap: null, capLabel: null, detail: t("sendFile.detailConnected", { os }) };
  });
  return { targets, loading };
}

export interface PrimaryAction {
  label: string;
  run: () => void;
  disabled: boolean;
}

/**
 * Estado derivado e ações de uma linha de transferência. `canOpen` diz se um
 * arquivo concluído pode ser aberto por fora (no Android a imagem do clipboard
 * fica em disco privado).
 */
export function useTransferRow(rec: TransferRecord, canOpen: boolean) {
  const devices = useApp((s) => s.devices);
  const selfId = useApp((s) => s.me?.deviceId ?? "");
  const busy = useApp((s) => !!s.transferBusy[rec.id]);
  const asked = useApp((s) => s.asks.includes(rec.id));
  const accept = useApp((s) => s.acceptTransfer);
  const decline = useApp((s) => s.declineTransfer);
  const cancel = useApp((s) => s.cancelTransfer);
  const resume = useApp((s) => s.resumeTransfer);
  const open = useApp((s) => s.openTransfer);

  const kind = rec.state.kind;
  const active = isTransferActive(rec);
  const incoming = rec.direction === "in";
  const primary: PrimaryAction | null = active
    ? (kind === "verifying" ? null : { label: busy ? t("transfers.cancelling") : t("transfers.cancel"), run: () => void cancel(rec.id), disabled: busy })
    : kind === "failed" && !incoming
      ? { label: t("common.retry"), run: () => void resume(rec.id), disabled: false }
      : kind === "completed" && canOpen
        ? { label: t("common.open"), run: () => void open(rec.id), disabled: false }
        : null;

  return {
    kind,
    active,
    incoming,
    busy,
    asked,
    dev: deviceName(devices, rec.deviceId, selfId),
    hasBar: ["transferring", "verifying", "waiting", "cancelled"].includes(kind),
    pct: kind === "verifying" ? 100 : percent(rec),
    primary,
    accept: () => void accept(rec.id),
    decline: () => void decline(rec.id),
    cancel: () => void cancel(rec.id),
    resume: () => void resume(rec.id),
    open: () => void open(rec.id),
  };
}
