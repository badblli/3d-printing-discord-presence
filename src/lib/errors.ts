import { translate, type Key } from './i18n';
import type { Lang } from './api';

const codes: Record<string, [Key, Key]> = {
  unreachable: ['unreachableTitle', 'unreachableHint'],
  auth_rejected: ['authTitle', 'authHint'], AuthRejected: ['authTitle', 'authHint'],
  subscribe_denied: ['subscribeTitle', 'subscribeHint'],
  stale: ['staleTitle', 'staleHint'], protocol: ['protocolTitle', 'protocolHint'],
  invalid_host: ['invalidHost', 'unreachableHint'], non_local_address: ['invalidHost', 'unreachableHint'],
  invalid_serial: ['invalidSerial', 'serialHelp'], invalid_access_code: ['invalidSecret', 'secretHelp'],
  invalid_secret: ['invalidSecret', 'secretHelp'], invalid_app_id: ['invalidAppId', 'appIdHelp'],
  secret_store_unavailable: ['storageTitle', 'storageHint'], secret_rollback_failed: ['storageTitle', 'storageHint'],
  config_io_failed: ['storageTitle', 'storageHint'], config_serialization_failed: ['storageTitle', 'storageHint'],
  legacy_unavailable: ['importTitle', 'importHint'], legacy_read_failed: ['importTitle', 'importHint'],
  legacy_decryption_failed: ['importTitle', 'importHint'],
  autostart_update_failed: ['autostartTitle', 'autostartErrorHint'], autostart_rollback_failed: ['autostartTitle', 'autostartErrorHint'],
};
/** Only known codes are rendered. Never display raw errors or credentials. */
export function errorMessage(error: unknown, lang: Lang) {
  const keys = typeof error === 'string' ? codes[error] : undefined;
  const [title, hint] = keys ?? ['genericTitle', 'genericHint'];
  return { title: translate(lang, title), hint: translate(lang, hint) };
}
