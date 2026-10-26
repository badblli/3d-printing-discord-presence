import { describe, expect, it } from 'vitest';
import { messages, tr, en } from './i18n';
import { errorMessage } from './errors';
import { validatePrinter, validAppId } from './forms';

const form = { name: 'Test printer', host: '192.168.1.50', serial: '00M00A000000000', secret: '12345678', model: 'A1' };

describe('printer validation', () => {
  it('accepts a fake local printer', () => expect(validatePrinter(form)).toEqual({}));
  it.each(['10.0.0.50', '172.16.0.50', '172.31.255.50', '169.254.1.50'])('accepts LAN address %s', host => {
    expect(validatePrinter({ ...form, host }).host).toBeUndefined();
  });
  it.each(['', '8.8.8.8', '127.0.0.1', '172.32.0.50', '192.168.1.256', '192.168.01.50', '192.168.1.50:8883', 'printer.local'])('rejects invalid or non-local address %s', host => {
    expect(validatePrinter({ ...form, host }).host).toBe('invalidHost');
  });
  it('requires a name and valid serial', () => {
    expect(validatePrinter({ ...form, name: ' ', serial: 'bad!' })).toMatchObject({ name: 'invalidName', serial: 'invalidSerial' });
  });
  it('requires an access code on creation', () => expect(validatePrinter({ ...form, secret: '' }).secret).toBe('invalidSecret'));
  it('preserves a saved secret when the field is empty', () => expect(validatePrinter({ ...form, secret: '' }, true)).toEqual({}));
  it('allows Studio to supply the code while still validating printer details', () => {
    expect(validatePrinter({ ...form, secret: '' }, false, true)).toEqual({});
    expect(validatePrinter({ ...form, secret: '', host: '', serial: '' }, false, true)).toMatchObject({ host: 'invalidHost', serial: 'invalidSerial' });
    expect(validatePrinter({ ...form, secret: '' }, false, false).secret).toBe('invalidSecret');
  });
  it.each(['short', '1234567!', '123456789', '1234567 '])('rejects an invalid replacement code', secret => {
    expect(validatePrinter({ ...form, secret }, true).secret).toBe('invalidSecret');
  });
  it('allows optional or numeric application IDs only', () => {
    expect(validAppId('')).toBe(true);
    expect(validAppId('123456789012345678')).toBe(true);
    expect(validAppId('token-do-not-use')).toBe(false);
    expect(validAppId('123')).toBe(false);
  });
});

describe('localized safe errors', () => {
  it.each(['tr', 'en'] as const)('maps connection errors and advice in %s', lang => {
    const expected = {
      unreachable: ['unreachableTitle', 'unreachableHint'], auth_rejected: ['authTitle', 'authHint'],
      subscribe_denied: ['subscribeTitle', 'subscribeHint'], stale: ['staleTitle', 'staleHint'], protocol: ['protocolTitle', 'protocolHint'],
    } as const;
    for (const [code, keys] of Object.entries(expected)) {
      expect(errorMessage(code, lang)).toEqual({ title: messages[lang][keys[0]], hint: messages[lang][keys[1]] });
    }
    expect(errorMessage('AuthRejected', lang)).toEqual(errorMessage('auth_rejected', lang));
    expect(errorMessage('auth_rejected', lang).hint).toContain('Developer Mode');
  });
  it('never displays unknown raw error content', () => {
    for (const error of ['private-input-test', new Error('private-input-test'), null, { secret: '12345678' }]) {
      expect(errorMessage(error, 'en')).toEqual({ title: en.genericTitle, hint: en.genericHint });
    }
  });
  it('offers specific storage, import and startup recovery', () => {
    expect(errorMessage('secret_store_unavailable', 'en').title).toBe(en.storageTitle);
    expect(errorMessage('legacy_decryption_failed', 'tr').hint).toBe(tr.importHint);
    expect(errorMessage('autostart_update_failed', 'en').hint).toBe(en.autostartErrorHint);
    expect(errorMessage('studio_code_missing', 'en')).toEqual({ title: en.studioMissingTitle, hint: en.studioMissingHint });
    expect(errorMessage('studio_code_missing', 'tr')).toEqual({ title: tr.studioMissingTitle, hint: tr.studioMissingHint });
  });
});

describe('i18n completeness', () => {
  it.each(['tr', 'en'] as const)('includes every discovery and Studio message in %s', lang => {
    for (const key of ['discoveredPrinters', 'scanning', 'rescan', 'discoveryEmpty', 'useStudioCode', 'studioMissingTitle', 'studioMissingHint'] as const) {
      expect(messages[lang][key].trim().length).toBeGreaterThan(0);
    }
  });
  it('has exactly the same keys in Turkish and English', () => expect(Object.keys(tr).sort()).toEqual(Object.keys(en).sort()));
  it.each(['tr', 'en'] as const)('has no blank messages in %s', lang => {
    for (const value of Object.values(messages[lang])) expect(value.trim().length).toBeGreaterThan(0);
  });
});
