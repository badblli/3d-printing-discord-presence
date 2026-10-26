import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/** UUIDs are serialized as strings. All numeric fields are JSON numbers. */
export type PrinterId = string;
export type Lang = 'tr' | 'en';
export type PrinterKind = 'bambu' | 'moonraker' | 'octo_print' | 'prusa_link';
export type PrintState = 'idle' | 'preparing' | 'printing' | 'paused' | 'finished' | 'failed';
export type ConnectionError = 'unreachable' | 'auth_rejected' | 'subscribe_denied' | 'stale' | 'protocol';
export type ConnectionState =
  | { state: 'connecting' | 'connected' | 'stopped' }
  | { state: 'error'; error: ConnectionError };

export type PrinterConnection =
  | { kind: 'bambu'; host: string; serial: string; model: string | null }
  | { kind: 'moonraker' | 'octo_print' | 'prusa_link'; url: string };

export interface PrinterConfig {
  id: PrinterId;
  name: string;
  enabled: boolean;
  connection: PrinterConnection;
}

export interface DiscordConfig {
  enabled: boolean;
  show_file_name: boolean;
  app_id_override: string | null;
  details_template: string | null;
  state_template: string | null;
  show_status_badge: boolean;
  buttons: DiscordButton[];
}

export interface DiscordButton {
  label: string;
  url: string;
}

export interface AppConfig {
  version: number;
  printers: PrinterConfig[];
  discord: DiscordConfig;
  autostart: boolean;
  lang: Lang;
}

export interface PrinterConfigView extends PrinterConfig {
  has_secret: boolean;
}

export interface ConfigView extends Omit<AppConfig, 'printers'> {
  printers: PrinterConfigView[];
}

/** Input only; never retained in config responses or snapshot events. */
export interface SecretUpdate {
  id: PrinterId;
  secret: string;
}

/** Contains availability only; Studio credentials never cross IPC. */
export interface DiscoveredPrinter {
  host: string;
  serial: string;
  name: string;
  model: string;
  studio_code: boolean;
}

export const discoverPrinters = (seconds?: number): Promise<DiscoveredPrinter[]> => invoke('discover_printers', { seconds });

export interface PrintStatus {
  kind: PrinterKind;
  state: PrintState;
  progress: number | null;
  remaining_secs: number | null;
  file_name: string | null;
  printer_model: string | null;
  layer: number | null;
  total_layers: number | null;
  nozzle_temp: number | null;
  bed_temp: number | null;
  observed_at_ms: number;
}

export interface Activity {
  details: string;
  state: string;
  assets: { large_image: string; large_text: string; small_image?: string; small_text?: string };
  /** Discord displays custom buttons to other viewers; the card owner cannot see their own buttons. */
  buttons?: DiscordButton[];
  /** Rust omits absent timestamps instead of returning null. */
  timestamps?: { end: number };
}

export type DiscordState = 'disabled' | 'no_app_id' | 'disconnected' | 'connected' | 'error';

export interface PrinterSnapshot {
  id: PrinterId;
  enabled: boolean;
  has_secret: boolean;
  unsupported: boolean;
  connection: ConnectionState;
  status: PrintStatus | null;
}

export interface Snapshot {
  printers: PrinterSnapshot[];
  discord: DiscordState;
  selected_printer: PrinterId | null;
  activity: Activity | null;
  legacy_available: boolean;
  recovered_corrupt: boolean;
}

export const getConfig = (): Promise<ConfigView> => invoke('get_config');
export const getSnapshot = (): Promise<Snapshot> => invoke('get_snapshot');

export type UpdateStatus =
  | { state: 'idle' | 'checking' | 'ready' }
  | { state: 'available'; version: string; notes: string | null }
  | { state: 'downloading'; percent: number }
  | { state: 'error'; code: 'check_failed' | 'download_failed' | 'install_failed' };

export interface UpdateSnapshot {
  current_version: string;
  last_checked_ms: number | null;
  status: UpdateStatus;
}

export const getUpdateState = (): Promise<UpdateSnapshot> => invoke('get_update_state');
export const checkForUpdates = (): Promise<UpdateSnapshot> => invoke('check_for_updates');
export const installUpdate = (): Promise<void> => invoke('install_update');
export function onUpdate(handler: (snapshot: UpdateSnapshot) => void): Promise<UnlistenFn> {
  return listen<UpdateSnapshot>('update', event => handler(event.payload));
}

export function saveConfig(config: AppConfig, secrets: SecretUpdate[] = [], studioSecrets: PrinterId[] = []): Promise<ConfigView> {
  // Strip the response-only flag even when the caller passes a ConfigView back.
  const input: AppConfig = {
    ...config,
    printers: config.printers.map(({ id, name, enabled, connection }) => ({ id, name, enabled, connection }))
  };
  return invoke('save_config', { config: input, secrets, studioSecrets });
}

export const deletePrinter = (id: PrinterId): Promise<ConfigView> => invoke('delete_printer', { id });
/** On other platforms, rejects with legacy_unavailable. */
export const importLegacy = (): Promise<ConfigView> => invoke('import_legacy');
export const setAutostart = (enabled: boolean): Promise<ConfigView> => invoke('set_autostart', { enabled });

/** Subscribe before reading getSnapshot to avoid missing the initial transition. */
export function onSnapshot(handler: (snapshot: Snapshot) => void): Promise<UnlistenFn> {
  return listen<Snapshot>('snapshot', (event) => handler(event.payload));
}
