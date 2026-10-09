import { locale, t, type Key } from "../i18n";
import type { ClipboardItem, ContentType, Device, LocalFile, SyncState, TransferKind, TransferRecord } from "./types";

/** Mapa `id → texto` cujos valores são lidos no idioma ativo a cada acesso (não congelam o idioma do load). */
function labels<K extends string>(prefix: string, ids: K[]): Record<K, string> {
  const out = {} as Record<K, string>;
  for (const id of ids) Object.defineProperty(out, id, { enumerable: true, get: () => t(`${prefix}.${id}` as Key) });
  return out;
}

export const TYPE_LABEL = labels<ContentType>("type", ["text", "link", "code", "command", "email", "token", "card", "image"]);

export const STATE_LABEL = labels<SyncState>("state", ["synced", "sending", "queued", "local"]);

/** Nomes próprios: não se traduzem. */
export const PLATFORM_LABEL: Record<string, string> = {
  windows: "Windows",
  android: "Android",
  macos: "macOS",
  linux: "Linux",
  ios: "iOS",
};

/** Tem "Abrir": e-mail, ou link que `open_item` aceita (http/https, ou www. que vira https). */
export function isOpenable(i: ClipboardItem) {
  return i.contentType === "email" || (i.contentType === "link" && /^\s*(https?:\/\/|www\.)/i.test(i.content));
}

export function isMono(t: ContentType) {
  return t !== "text" && t !== "image";
}

/** "Imagem · 1920×1080 · 2,4 MB" a partir do `metadata` do item. */
export function imageLabel(it: ClipboardItem): string {
  const m = it.metadata as { width?: number; height?: number; bytes?: number };
  return [t("type.image"), m.width && m.height ? `${m.width}×${m.height}` : null, m.bytes ? bytes(m.bytes) : null].filter(Boolean).join(" · ");
}

export function relTime(ts: number | null | undefined, now = Date.now()): string {
  if (!ts) return "—";
  const diff = Math.max(0, now - ts);
  const s = Math.round(diff / 1000);
  if (s < 45) return t("time.now");
  const m = Math.round(s / 60);
  if (m < 60) return t("time.minAgo", { n: m });
  const h = Math.round(m / 60);
  if (h < 24) return t("time.hourAgo", { n: h });
  const d = Math.round(h / 24);
  if (d === 1) return t("time.yesterday");
  if (d < 30) return t("time.dayAgo", { n: d });
  return new Date(ts).toLocaleDateString(locale());
}

export function shortTime(ts: number | null | undefined, now = Date.now()): string {
  if (!ts) return "—";
  const diff = Math.max(0, now - ts);
  const m = Math.round(diff / 60000);
  if (m < 1) return t("time.now");
  if (m < 60) return t("unit.min", { n: m });
  const h = Math.round(m / 60);
  if (h < 24) return t("unit.h", { n: h });
  return t("unit.d", { n: Math.round(h / 24) });
}

export function countdown(expiresAt: number | null, now = Date.now()): string {
  if (!expiresAt) return "";
  const left = Math.max(0, expiresAt - now);
  const m = Math.floor(left / 60000);
  const s = Math.floor((left % 60000) / 1000);
  if (m >= 60) return t("unit.h", { n: Math.round(m / 60) });
  if (m > 0) return t("unit.min", { n: m });
  return t("unit.s", { n: s });
}

export function mmss(ms: number): string {
  const t = Math.max(0, Math.floor(ms / 1000));
  return `${String(Math.floor(t / 60)).padStart(2, "0")}:${String(t % 60).padStart(2, "0")}`;
}

export const MASK = "•••••••••••••••••••••";

export function firstLine(s: string, max = 140): string {
  const line = s.split(/\r?\n/).find((l) => l.trim()) ?? "";
  return line.length > max ? line.slice(0, max) + "…" : line;
}

export function deviceName(devices: Device[], id: string, selfId: string): string {
  if (id === selfId) {
    const me = devices.find((d) => d.isSelf);
    return me?.name ?? t("device.thisDevice");
  }
  return devices.find((d) => d.deviceId === id)?.name ?? t("device.removed");
}

export function itemNote(it: ClipboardItem, revealed: boolean, now: number): string {
  if (it.isSensitive) {
    const c = countdown(it.expiresAt, now);
    return `${t(revealed ? "item.visible" : "item.hidden")}${c ? ` · ${t("item.expiresIn", { time: c })}` : ""}`;
  }
  return "";
}

export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return `${v.toLocaleString(locale(), { maximumFractionDigits: v < 10 ? 1 : 0 })} ${units[i]}`;
}

export function percent(t: TransferRecord): number {
  if (t.size === 0) return 100;
  return Math.min(100, Math.floor((t.bytesDone / t.size) * 100));
}

export const TRANSFER_LABEL = labels<TransferKind>("transferState", [
  "queued", "offered", "incoming", "transferring", "verifying", "waiting", "completed", "cancelled", "failed",
]);

export const TRANSFER_TAG: Record<TransferKind, string> = {
  queued: "tag-queued",
  offered: "tag-sending",
  incoming: "tag-sending",
  transferring: "tag-sending",
  verifying: "tag-synced",
  waiting: "tag-sending",
  completed: "tag-synced",
  cancelled: "tag-queued",
  failed: "tag-failed",
};

export const TRANSFER_ACTIVE: TransferKind[] = ["queued", "offered", "incoming", "transferring", "verifying", "waiting"];

export function isTransferActive(t: TransferRecord) {
  return TRANSFER_ACTIVE.includes(t.state.kind);
}

/** Mensagem legível para `Failed { error }` — o backend manda códigos curtos. */
export function transferError(error: string, device: string): string {
  if (error.startsWith("rejected:")) {
    const why = error.slice("rejected:".length);
    switch (why) {
      case "noSpace": return t("transferError.noSpace", { device });
      case "busy": return t("transferError.busy", { device });
      case "declined": return t("transferError.declined", { device });
      case "unsupported": return t("transferError.unsupported", { device });
      default: return t("transferError.rejected", { device, why });
    }
  }
  if (error.startsWith("checksum:")) return t("transferError.checksum");
  if (error.includes("permission") || error.includes("Permission")) return t("transferError.permission");
  return error;
}

/** Imagem do clipboard (vai para a área de transferência, não para Downloads). */
export const isClipImage = (t: TransferRecord) => t.purpose === "clipboard";

/** Imagem do clipboard concluída já está no histórico: sai das listas de Arquivos (ativas e falhas ficam). */
export const isHiddenTransfer = (t: TransferRecord) => isClipImage(t) && t.state.kind === "completed";

export function transferDetail(rec: TransferRecord, device: string): string {
  const pct = percent(rec);
  const size = bytes(rec.size);
  switch (rec.state.kind) {
    case "transferring": return `${bytes(rec.bytesDone)} / ${size} · ${pct}%`;
    case "verifying": return t("transferDetail.verifying");
    case "waiting": return t("transferDetail.waiting", { done: bytes(rec.bytesDone), size, device });
    case "queued": return t("transferDetail.queued", { device });
    case "offered": return t("transferDetail.offered", { device });
    case "incoming": return t("transferDetail.incoming", { size });
    case "completed":
      if (isClipImage(rec)) return t(rec.direction === "in" ? "transferDetail.clipIn" : "transferDetail.clipOut", { size });
      return t(rec.direction === "in" ? "transferDetail.savedIn" : "transferDetail.sentOut", { size });
    case "cancelled": return t("transferDetail.cancelled", { pct, size });
    case "failed": return transferError(rec.state.error, device);
  }
}

/** Título e linha de detalhe de uma seleção para enviar (pastas vão como .zip). */
export function filesSummary(files: LocalFile[]): { title: string; meta: string } {
  const dirs = files.filter((f) => f.dir).length;
  const total = files.reduce((n, f) => n + (f.dir ? 0 : f.size), 0);
  const title = files.length === 1
    ? files[0].name
    : t(dirs > 0 ? "files.items" : "files.files", { n: files.length });
  const meta = [
    total > 0 ? bytes(total) : null,
    dirs > 0 ? (dirs === 1 && files.length === 1 ? t("files.oneFolderZip") : t("files.foldersZip", { n: dirs })) : null,
  ].filter(Boolean).join(" · ");
  return { title, meta };
}
