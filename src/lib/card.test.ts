import { describe, expect, it } from 'vitest';
import type { PrintStatus } from './api';
import { countdown, defaultDetails, defaultState, placeholders, renderTemplate, validButtons, validTemplate } from './card';

const status: PrintStatus = { kind: 'bambu', state: 'printing', progress: 42, remaining_secs: 4800,
  file_name: '/fake/example.gcode.3mf', printer_model: 'Bambu Lab A1', layer: 12, total_layers: 200,
  nozzle_temp: 215, bed_temp: 60, observed_at_ms: 1000000 };
describe('card template preview matches the presence formatter', () => {
  it('formats defaults and every placeholder in both languages', () => {
    expect(renderTemplate(defaultDetails, status, 'en', true)).toBe('example');
    expect(renderTemplate(defaultState, status, 'tr', true)).toBe('Bambu Lab A1 · Basılıyor · %42');
    expect(renderTemplate(defaultState, status, 'en', true)).toBe('Bambu Lab A1 · Printing · 42%');
    expect(renderTemplate(placeholders.map(key => `{${key}}`).join(' · '), status, 'en', true))
      .toBe('example · Bambu Lab A1 · Printing · 42% · 12 · 200 · 215°C · 60°C · 1h 20m');
    expect(renderTemplate('{remaining}', status, 'tr', true)).toBe('1 sa 20 dk');
  });
  it('normalizes percent wrappers and handles zero', () => {
    for (const template of ['{progress}', '%{progress}', '{progress}%']) {
      expect(renderTemplate(template, { ...status, progress: 0 }, 'en', true)).toBe('0%');
      expect(renderTemplate(template, status, 'tr', true)).toBe('%42');
    }
  });
  it('drops missing segments, falls back, and bounds Unicode output', () => {
    expect(renderTemplate('{model} · Layer {layer}/{total_layers} · {nozzle}', { ...status, layer: null, nozzle_temp: Infinity }, 'en', true)).toBe('Bambu Lab A1');
    expect(renderTemplate('{layer}', { ...status, layer: null }, 'tr', true)).toBe('3D baskı alıyorum');
    expect(Array.from(renderTemplate('🖨'.repeat(200), status, 'en', true))).toHaveLength(128);
  });
  it('respects file privacy and never reparses telemetry as templates', () => {
    expect(renderTemplate('{file}', status, 'en', false)).toBe('3D printing');
    expect(renderTemplate('{file}', { ...status, file_name: '{nozzle}.gcode' }, 'en', true)).toBe('{nozzle}');
  });
  it.each(['{', '}', '{{file}}', '{unknown}', '{}', '{file}}'])('rejects malformed template %s', template => {
    expect(validTemplate(template)).toBe(false);
    expect(renderTemplate(template, status, 'en', true)).toBe('3D printing');
  });
});
describe('Discord link validation', () => {
  const button = { label: 'Fake link', url: 'https://example.com/model?id=42' };
  it('accepts zero through two buttons and Unicode labels', () => {
    expect(validButtons([])).toBe(true);
    expect(validButtons([button, { ...button, label: '🖨'.repeat(32) }])).toBe(true);
    expect(validButtons([button, button, button])).toBe(false);
  });
  it.each(['', ' ', 'x\n', 'x'.repeat(33)])('rejects invalid labels', label => expect(validButtons([{ ...button, label }])).toBe(false));
  it.each(['http://example.com', 'javascript:alert(1)', 'https://', 'https://user:password@example.com', 'https://example.com\n', 'https://example.com/a b', 'https://example.com\\path'])('rejects invalid URLs', url => expect(validButtons([{ ...button, url }])).toBe(false));
});
describe('timestamp countdown', () => {
  it('counts down from the deadline and clamps expired values', () => {
    expect(countdown(5800, 1000)).toBe('01:20:00');
    expect(countdown(5800, 1001)).toBe('01:19:59');
    expect(countdown(1000, 5800)).toBe('00:00:00');
  });
});
