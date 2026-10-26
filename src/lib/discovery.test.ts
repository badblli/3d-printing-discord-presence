import { beforeEach, describe, expect, it, vi } from 'vitest';
import { discoverPrinters, saveConfig, type AppConfig } from './api';
import { render } from 'svelte/server';
import PrinterWizard from './components/PrinterWizard.svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';

const config: AppConfig = {
  version: 1, printers: [], autostart: false, lang: 'en',
  discord: { enabled: true, show_file_name: false, app_id_override: null, details_template: null,
    state_template: null, show_status_badge: true, buttons: [] },
};

describe('discovery IPC', () => {
  beforeEach(() => vi.mocked(invoke).mockReset());
  it('requests the default scan duration or an explicit duration', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await discoverPrinters();
    expect(invoke).toHaveBeenLastCalledWith('discover_printers', { seconds: undefined });
    await discoverPrinters(1);
    expect(invoke).toHaveBeenLastCalledWith('discover_printers', { seconds: 1 });
  });
  it('passes only printer IDs when requesting Studio credentials', async () => {
    vi.mocked(invoke).mockResolvedValue(config);
    await saveConfig(config, [], ['00000000-0000-0000-0000-000000000001']);
    expect(invoke).toHaveBeenCalledWith('save_config', {
      config, secrets: [], studioSecrets: ['00000000-0000-0000-0000-000000000001'],
    });
    await saveConfig(config);
    expect(invoke).toHaveBeenLastCalledWith('save_config', { config, secrets: [], studioSecrets: [] });
  });
  it('keeps editing manual without starting discovery', () => {
    const printer = { id: '00000000-0000-0000-0000-000000000001', name: 'Fake printer', enabled: true, has_secret: true,
      connection: { kind: 'bambu' as const, host: '192.168.1.50', serial: '00M00A000000000', model: null } };
    const { body } = render(PrinterWizard, { props: {
      config: { ...config, printers: [printer] }, editing: printer,
      snapshot: { printers: [], discord: 'disconnected', selected_printer: null, activity: null, legacy_available: false, recovered_corrupt: false },
      onconfig: () => {}, onclose: () => {},
    } });
    expect(body).toContain('id="printer-secret"');
    expect(body).toContain('Saved · enter a new code to replace');
    expect(body).not.toContain('discovery-card');
    expect(invoke).not.toHaveBeenCalled();
  });
});
