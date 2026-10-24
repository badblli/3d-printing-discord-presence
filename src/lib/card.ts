import type { DiscordButton, Lang, PrintStatus } from './api';

export const placeholders = ['file', 'model', 'status', 'progress', 'layer', 'total_layers', 'nozzle', 'bed', 'remaining'] as const;
export const defaultDetails = '{file}';
export const defaultState = '{model} · {status} · {progress}';
const chars = (text: string, limit: number) => Array.from(text).slice(0, limit).join('');
export function validTemplate(text: string): boolean {
  return !text.replace(/\{([^{}]+)\}/g, (token, key) => placeholders.includes(key) ? '' : token).match(/[{}]/);
}
export function validButtons(buttons: DiscordButton[]): boolean {
  return buttons.length <= 2 && buttons.every(({ label, url }) => {
    if (!label.trim() || Array.from(label).length > 32 || /[\x00-\x1f\x7f-\x9f]/.test(label)) return false;
    if (!url.startsWith('https://') || /[\s\x00-\x1f\x7f-\x9f\\]/.test(url)) return false;
    try { const parsed = new URL(url); return !!parsed.hostname && !parsed.username && !parsed.password; }
    catch { return false; }
  });
}
export function countdown(end: number, now: number): string {
  const seconds = Math.max(0, Math.ceil(end - now));
  return `${Math.floor(seconds / 3600).toString().padStart(2, '0')}:${(Math.floor(seconds / 60) % 60).toString().padStart(2, '0')}:${(seconds % 60).toString().padStart(2, '0')}`;
}
/** Mirrors presence/template.rs: one pass, missing values drop their · segment. */
export function renderTemplate(text: string, status: PrintStatus, lang: Lang, showFile: boolean): string {
  const english = lang === 'en';
  const fallback = english ? '3D printing' : '3D baskı alıyorum';
  if (!validTemplate(text)) return fallback;
  const file = chars((status.file_name ?? '').trim().split(/[/\\]/).pop()!.trim().replace(/(?:\.gcode\.3mf|\.gcode|\.3mf)$/i, '').trim(), 100);
  const model = status.printer_model?.trim() ?? '';
  const minutes = Math.ceil((status.remaining_secs ?? 0) / 60);
  const temp = (value: number | null) => value !== null && Number.isFinite(value) ? `${Math.round(value)}°C` : null;
  const values: Record<string, string | null> = {
    file: showFile && Array.from(file).length >= 2 ? file : fallback,
    model: Array.from(model).length >= 2 ? chars(model, 80) : english ? '3D printer' : '3D yazıcı',
    status: ({ preparing: english ? 'Preparing' : 'Hazırlanıyor', printing: english ? 'Printing' : 'Basılıyor', paused: english ? 'Paused' : 'Duraklatıldı' } as Record<string, string>)[status.state] ?? '',
    progress: status.progress !== null && status.progress <= 100 ? english ? `${status.progress}%` : `%${status.progress}` : null,
    layer: status.layer?.toString() ?? null, total_layers: status.total_layers?.toString() ?? null,
    nozzle: temp(status.nozzle_temp), bed: temp(status.bed_temp),
    remaining: status.remaining_secs === null ? null : minutes < 60 ? `${minutes}${english ? 'm' : ' dk'}` : `${Math.floor(minutes / 60)}${english ? 'h ' : ' sa '}${minutes % 60}${english ? 'm' : ' dk'}`,
  };
  const result = text.replace(/%\{progress\}|\{progress\}%/g, '{progress}').split(' · ').map(segment => {
    let missing = false;
    const rendered = segment.replace(/\{([^{}]+)\}/g, (_, key: string) => { if (values[key] === null) missing = true; return values[key] ?? ''; });
    return missing ? '' : rendered.trim();
  }).filter(Boolean).join(' · ');
  return chars(Array.from(result).length >= 2 ? result : fallback, 128);
}
