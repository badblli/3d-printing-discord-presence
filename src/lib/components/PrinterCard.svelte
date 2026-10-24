<script lang="ts">
  import type { Lang, PrinterConfigView, PrinterSnapshot } from '../api';
  import { translate, type Key } from '../i18n';
  import { errorMessage } from '../errors';
  export let printer: PrinterConfigView;
  export let snapshot: PrinterSnapshot | undefined;
  export let lang: Lang;
  $: t = (key: Key) => translate(lang, key);
  $: status = snapshot?.status;
  $: progress = status?.progress == null ? null : Math.min(100, Math.max(0, status.progress));
  $: connection = snapshot?.connection;
  $: problem = errorMessage(connection?.state === 'error' ? connection.error : null, lang);
  function remaining(seconds: number | null | undefined) {
    if (seconds == null) return t('unknown');
    const minutes = Math.ceil(Math.max(0, seconds) / 60);
    return minutes >= 60 ? `${Math.floor(minutes / 60)} ${t('hours')} ${minutes % 60} ${t('minutes')}` : `${minutes} ${t('minutes')}`;
  }
</script>

<article class="panel printer-card">
  <div class="card-heading"><div><p class="eyebrow">{t(printer.connection.kind)}</p><h2>{printer.name}</h2></div><span class="badge" class:online={connection?.state === 'connected'}><i></i>{snapshot?.unsupported ? t('soon') : t(connection?.state ?? 'connecting')}</span></div>
  <div class="print-state"><strong>{status ? t(status.state) : t('noData')}</strong><span class="percentage">{progress == null ? t('unknown') : `${Math.round(progress)}${t('percent')}`}</span></div>
  <progress max="100" value={progress ?? 0} aria-label={t('progress')}></progress>
  <div class="telemetry"><span>{remaining(status?.remaining_secs)} <small>{t('remaining')}</small></span><span><small>{t('nozzle')}</small> {status?.nozzle_temp == null ? t('unknown') : Math.round(status.nozzle_temp) + t('temperature')}</span><span><small>{t('bed')}</small> {status?.bed_temp == null ? t('unknown') : Math.round(status.bed_temp) + t('temperature')}</span></div>
  {#if connection?.state === 'error'}<details class="connection-error"><summary>{problem.title}</summary><p>{problem.hint}</p></details>{/if}
</article>
