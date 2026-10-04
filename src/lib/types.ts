export type ContentType = "text" | "link" | "code" | "command" | "email" | "token" | "card";
export type SyncState = "synced" | "sending" | "queued" | "failed" | "local";
export type DeviceState =
  | "discovered"
  | "pairing"
  | "paired"
  | "connecting"
  | "connected"
  | "disconnected"
  | "blocked";
export type Platform = "windows" | "android" | "macos" | "linux" | "ios";

export interface ClipboardItem {
  id: string;
  content: string;
  contentType: ContentType;
  sourceDeviceId: string;
  createdAt: number;
  expiresAt: number | null;
  isPinned: boolean;
  isSensitive: boolean;
  syncState: SyncState;
  metadata: Record<string, unknown>;
}

export interface Device {
  deviceId: string;
  name: string;
  platform: string;
  publicKey: string;
  state: DeviceState;
  isSelf: boolean;
  autoSend: boolean;
  autoReceive: boolean;
  lastSeen: number | null;
  address: string | null;
  pairedAt: number | null;
  lastError: string | null;
  queued: number;
}

export interface DeviceIdentity {
  deviceId: string;
  name: string;
  platform: string;
  publicKey: string;
}

export interface Settings {
  deviceName: string;
  autoSend: boolean;
  autoReceive: boolean;
  syncedTypes: string[];
  /** Arquivo recebido: "auto" | "limit" (aceita até fileAutoAcceptMb) | "ask". */
  fileAccept: "auto" | "limit" | "ask";
  fileAutoAcceptMb: number;
  /** Toasts do Windows (arquivo recebido, pedido de aceite). */
  notifications: boolean;
  notifyText: boolean;
  typesRev: number;
  lanOnly: boolean;
  ignorePasswordManagers: boolean;
  autoExpireHours: number;
  hideSensitive: boolean;
  sensitiveExpireMinutes: number;
  launchAtLogin: boolean;
  autostartInitialized: boolean;
}

export interface SyncStatus {
  paused: boolean;
  pausedUntil: number | null;
  online: boolean;
  connectedCount: number;
  queuedCount: number;
  lastError: string | null;
}

export interface Bootstrap {
  platform: Platform;
  me: DeviceIdentity;
  settings: Settings;
  devices: Device[];
  items: ClipboardItem[];
  sync: SyncStatus;
  listenPort: number;
  totalItems: number;
  pinnedItems: number;
  onboarded: boolean;
  keyboardPromptSeen: boolean;
}

export interface ImeTheme {
  id: string;
  label: string;
}

export interface ImeStatus {
  supported: boolean;
  enabled: boolean;
  selected: boolean;
  theme: string;
  themes: ImeTheme[];
}

export interface ItemsPage {
  items: ClipboardItem[];
  total: number;
  pinned: number;
}

export interface PairingInfo {
  pairingId: string;
  code: string;
  qrSvg: string;
  startedAt: number;
  expiresAt: number;
  payload: string;
}

export type Filter = "all" | "text" | "code" | "link" | "email" | "sensitive" | "pinned";

// Espelho de `transfer::state::TransferState` (serde tag = "kind") e `TransferRecord`.
export type TransferKind =
  | "queued"
  | "offered"
  | "incoming"
  | "transferring"
  | "verifying"
  | "waiting"
  | "completed"
  | "failed"
  | "cancelled";

export type TransferState =
  | { kind: "queued" }
  | { kind: "offered" }
  | { kind: "incoming" }
  | { kind: "transferring" }
  | { kind: "verifying" }
  | { kind: "waiting"; reason: string }
  | { kind: "completed" }
  | { kind: "failed"; error: string }
  | { kind: "cancelled"; by: "local" | "remote" | "system" };

export interface TransferRecord {
  id: string;
  direction: "out" | "in";
  deviceId: string;
  originalName: string;
  safeName: string;
  mime: string | null;
  size: number;
  chunkSize: number;
  sha256Expected: string | null;
  sha256Actual: string | null;
  state: TransferState;
  bytesDone: number;
  source: string | null;
  tempPath: string | null;
  finalPath: string | null;
  attempts: number;
  createdAt: number;
  updatedAt: number;
  completedAt: number | null;
  /** "clipboard" = imagem copiada (vai para o clipboard, não para Downloads). */
  purpose: string | null;
}

/** Permissão de notificação do Android 13+ (`notify_permission`). */
export type NotifyPermission = "granted" | "prompt" | "blocked" | "unsupported";

/** "Compartilhar → ClipSync" pendente (`share_take`, só Android). */
export interface SharedPayload {
  files: { uri: string; name: string; size: number; mime: string | null }[];
  text: string | null;
}

/** Arquivo ou pasta (PC; vai como .zip) a enviar. `path` é caminho local ou `content://` URI. */
export interface LocalFile {
  path: string;
  name: string;
  size: number;
  dir?: boolean;
}

/** Arquivos copiados no Explorer (`clipboard:files`, só Windows): o app oferece enviar. */
export interface CopiedFiles {
  files: LocalFile[];
  skippedFiles: number;
  copiedAt: number;
}

/** Resultado do seletor nativo (`transfer_pick_file`). */
export type PickedFiles =
  | { cancelled: true }
  | { cancelled?: false; files: { uri: string; name: string; size: number; mime: string | null; dir?: boolean }[] };
