import type { ClipboardItem, ContentType, Device, LocalFile, SyncState, TransferKind, TransferRecord } from "./types";

export const TYPE_LABEL: Record<ContentType, string> = {
  text: "Texto",
  link: "Link",
  code: "Código",
  command: "Comando",
  email: "E-mail",
  token: "Token",
  card: "Cartão",
};

export const STATE_LABEL: Record<SyncState, string> = {
  synced: "Sincronizado",
  sending: "Enviando",
  queued: "Na fila",
  local: "Local",
};

export const PLATFORM_LABEL: Record<string, string> = {
  windows: "Windows",
  android: "Android",
  macos: "macOS",
  linux: "Linux",
  ios: "iOS",
};

export function isMono(t: ContentType) {
  return t !== "text";
}

export function relTime(ts: number | null | undefined, now = Date.now()): string {
  if (!ts) return "—";
  const diff = Math.max(0, now - ts);
  const s = Math.round(diff / 1000);
  if (s < 45) return "agora";
  const m = Math.round(s / 60);
  if (m < 60) return `há ${m} min`;
  const h = Math.round(m / 60);
  if (h < 24) return `há ${h} h`;
  const d = Math.round(h / 24);
  if (d === 1) return "ontem";
  if (d < 30) return `há ${d} d`;
  return new Date(ts).toLocaleDateString("pt-BR");
}

export function shortTime(ts: number | null | undefined, now = Date.now()): string {
  if (!ts) return "—";
  const diff = Math.max(0, now - ts);
  const m = Math.round(diff / 60000);
  if (m < 1) return "agora";
  if (m < 60) return `${m} min`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h`;
  return `${Math.round(h / 24)} d`;
}

export function countdown(expiresAt: number | null, now = Date.now()): string {
  if (!expiresAt) return "";
  const left = Math.max(0, expiresAt - now);
  const m = Math.floor(left / 60000);
  const s = Math.floor((left % 60000) / 1000);
  if (m >= 60) return `${Math.round(m / 60)} h`;
  if (m > 0) return `${m} min`;
  return `${s} s`;
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
    return me?.name ?? "Este dispositivo";
  }
  return devices.find((d) => d.deviceId === id)?.name ?? "Dispositivo removido";
}

export function itemNote(it: ClipboardItem, revealed: boolean, now: number): string {
  if (it.isSensitive) {
    const c = countdown(it.expiresAt, now);
    return `${revealed ? "visível" : "oculto"}${c ? ` · expira em ${c}` : ""}`;
  }
  return "";
}

export function pluralize(n: number, one: string, many: string) {
  return `${n} ${n === 1 ? one : many}`;
}

export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return `${v.toLocaleString("pt-BR", { maximumFractionDigits: v < 10 ? 1 : 0 })} ${units[i]}`;
}

export function percent(t: TransferRecord): number {
  if (t.size === 0) return 100;
  return Math.min(100, Math.floor((t.bytesDone / t.size) * 100));
}

export const TRANSFER_LABEL: Record<TransferKind, string> = {
  queued: "Na fila",
  offered: "Aguardando aceite",
  incoming: "Aguardando aceite",
  transferring: "Transferindo",
  verifying: "Verificando",
  waiting: "Aguardando conexão",
  completed: "Concluído",
  cancelled: "Cancelado",
  failed: "Falhou",
};

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
      case "noSpace": return `Espaço insuficiente em ${device}`;
      case "busy": return `${device} está ocupado com outras transferências`;
      case "declined": return `${device} recusou o arquivo`;
      case "unsupported": return `${device} não aceita arquivos`;
      default: return `${device} recusou — ${why}`;
    }
  }
  if (error.startsWith("checksum:")) return "Checksum inválido — o arquivo recebido foi descartado";
  if (error.includes("permission") || error.includes("Permission")) return "Permissão do arquivo revogada — selecione o arquivo novamente";
  return error;
}

/** Imagem do clipboard (vai para a área de transferência, não para Downloads). */
export const isClipImage = (t: TransferRecord) => t.purpose === "clipboard";

export function transferDetail(t: TransferRecord, device: string): string {
  const pct = percent(t);
  switch (t.state.kind) {
    case "transferring": return `${bytes(t.bytesDone)} / ${bytes(t.size)} · ${pct}%`;
    case "verifying": return "Verificando integridade do arquivo…";
    case "waiting": return `${bytes(t.bytesDone)} / ${bytes(t.size)} · retoma quando ${device} voltar`;
    case "queued": return `Aguardando conexão com ${device}`;
    case "offered": return `Aguardando ${device} aceitar`;
    case "incoming": return `${bytes(t.size)} · aguardando aceite`;
    case "completed":
      if (isClipImage(t)) return t.direction === "in" ? `${bytes(t.size)} · imagem copiada recebida` : `${bytes(t.size)} · imagem copiada enviada`;
      return t.direction === "in" ? `${bytes(t.size)} · salvo em Downloads` : `${bytes(t.size)} · enviado`;
    case "cancelled": return `Cancelado em ${pct}% · ${bytes(t.size)}`;
    case "failed": return transferError(t.state.error, device);
  }
}

/** Título e linha de detalhe de uma seleção para enviar (pastas vão como .zip). */
export function filesSummary(files: LocalFile[]): { title: string; meta: string } {
  const dirs = files.filter((f) => f.dir).length;
  const total = files.reduce((n, f) => n + (f.dir ? 0 : f.size), 0);
  const title = files.length === 1
    ? files[0].name
    : dirs > 0 ? pluralize(files.length, "item", "itens") : pluralize(files.length, "arquivo", "arquivos");
  const meta = [
    total > 0 ? bytes(total) : null,
    dirs > 0 ? (dirs === 1 && files.length === 1 ? "pasta · vai como .zip" : `${pluralize(dirs, "pasta vai", "pastas vão")} como .zip`) : null,
  ].filter(Boolean).join(" · ");
  return { title, meta };
}
