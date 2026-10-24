import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import UpdateBanner from './components/UpdateBanner.svelte';
import Preferences from './components/Preferences.svelte';
import type { ConfigView, UpdateSnapshot } from './api';

const snapshot = (status: UpdateSnapshot['status']): UpdateSnapshot => ({
  current_version: '0.2.0', last_checked_ms: 0, status
});
const config: ConfigView = {
  version: 1, printers: [], autostart: false, lang: 'tr',
  discord: { enabled: true, show_file_name: false, app_id_override: null,
    details_template: null, state_template: null, show_status_badge: false, buttons: [] }
};

describe('update banner', () => {
  it('offers a restart only for an available release', () => {
    const { body } = render(UpdateBanner, { props: { lang: 'tr', initial: snapshot({ state: 'available', version: '0.3.0', notes: null }) } });
    expect(body).toContain('Yeni sürüm v0.3.0');
    expect(body).toContain('Güncelle ve yeniden başlat');
    expect(body).toContain('Sonra');
  });
  it('shows progress without another install action', () => {
    const { body } = render(UpdateBanner, { props: { lang: 'en', initial: snapshot({ state: 'downloading', percent: 42 }) } });
    expect(body).toContain('Downloading update 42%');
    expect(body).toContain('value="42"');
    expect(body).not.toContain('Update and restart');
  });
  it('stays hidden while idle or checking', () => {
    for (const state of ['idle', 'checking'] as const) {
      expect(render(UpdateBanner, { props: { lang: 'en', initial: snapshot({ state }) } }).body).not.toContain('update-banner');
    }
  });
});

describe('preferences updates section', () => {
  it('shows installed version, last check and manual control', () => {
    const { body } = render(Preferences, { props: { config, onconfig: () => {}, onedit: () => {}, onadd: () => {}, initialTab: 'updates', initialUpdate: snapshot({ state: 'idle' }) } });
    expect(body).toContain('Güncellemeler');
    expect(body).toContain('Mevcut sürüm');
    expect(body).toContain('v0.2.0');
    expect(body).toContain('Henüz denetlenmedi');
    expect(body).toContain('Şimdi denetle');
  });
});
