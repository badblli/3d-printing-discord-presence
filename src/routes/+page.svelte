<script lang="ts">
  import logo from '$lib/assets/logo.png';
  import { onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import ActivityPreview from '$lib/components/ActivityPreview.svelte';
  import { getConfig, getSnapshot, onSnapshot, type ConfigView, type Snapshot, type PrinterConfigView } from '$lib/api';
  import { translate, type Key } from '$lib/i18n';
  import { errorMessage } from '$lib/errors';
  import PrinterWizard from '$lib/components/PrinterWizard.svelte';
  import PrinterCard from '$lib/components/PrinterCard.svelte';
  import Preferences from '$lib/components/Preferences.svelte';
  import UpdateBanner from '$lib/components/UpdateBanner.svelte';
  import '$lib/theme.css';

  let config: ConfigView | null = null;
  let snapshot: Snapshot | null = null;
  let view: 'status' | 'settings' = 'status';
  let wizard = false;
  let editing: PrinterConfigView | null = null;
  let failure: unknown = null;
  let loading = true;
  let disposed = false;
  let unsubscribe: (() => void) | undefined;
  let generation = 0;
  let printerPage = 0;
  $: if (config && printerPage >= config.printers.length) printerPage = 0;
  async function windowAction(action: 'minimize' | 'close') {
    try { await getCurrentWindow()[action](); } catch (error) { failure = error; }
  }
  $: lang = config?.lang ?? 'tr';
  $: if (typeof document !== 'undefined') document.documentElement.lang = lang;
  $: t = (key: Key) => translate(lang, key);
  $: problem = errorMessage(failure, lang);
  $: discordKey = (snapshot?.discord === 'connected' ? 'discordConnected' : snapshot?.discord === 'error' ? 'discordError' : snapshot?.discord ?? 'disconnected') as Key;

  async function initialize() {
    loading = true; failure = null;
    unsubscribe?.(); unsubscribe = undefined;
    const current = ++generation;
    let received = false;
    try {
      const stop = await onSnapshot(value => {
        if (!disposed && current === generation) { received = true; snapshot = value; }
      });
      if (disposed || current !== generation) { stop(); return; }
      unsubscribe = stop;
      const [settings, initial] = await Promise.all([getConfig(), getSnapshot()]);
      if (disposed || current !== generation) return;
      config = settings;
      if (!received) snapshot = initial;
      wizard = settings.printers.length === 0;
    } catch (error) { if (!disposed) failure = error; }
    finally {
      if (!disposed) { loading = false; await tick(); await invoke('ui_ready').catch(error => { failure = error; }); }
    }
  }
  onMount(() => { void initialize(); return () => { disposed = true; generation++; unsubscribe?.(); }; });
  function update(value: ConfigView) { config = value; }
  function add(printer: PrinterConfigView | null = null) { editing = printer; wizard = true; }
</script>

<svelte:window on:contextmenu|preventDefault />
<svelte:head><title>{t('app')}</title></svelte:head>
<div class="app-shell">
  <header data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region><img data-tauri-drag-region src={logo} alt="" width="44" height="44" /><div data-tauri-drag-region><strong data-tauri-drag-region>{t('app')}</strong><small data-tauri-drag-region>{t('tagline')}</small></div></div>
    {#if config && !wizard}<nav aria-label={t('app')}><button class:current={view === 'status'} aria-current={view === 'status' ? 'page' : undefined} on:click={() => view = 'status'}>{t('status')}</button><button class:current={view === 'settings'} aria-current={view === 'settings' ? 'page' : undefined} on:click={() => view = 'settings'}>{t('settings')}</button></nav>{/if}
    <div class="window-controls">
      <button aria-label={t('minimize')} title={t('minimize')} on:click={() => windowAction('minimize')}><svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8h10"/></svg></button>
      <button aria-label={t('hideToTray')} title={t('hideToTray')} on:click={() => windowAction('close')}><svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8M12 4l-8 8"/></svg></button>
    </div>
  </header>
  <main>
    {#if loading}<section class="panel empty" role="status"><img src={logo} alt="" width="96" height="96" /><p>{t('loading')}</p></section>
    {:else if failure}<section class="panel empty" role="alert"><h1>{problem.title}</h1><p>{problem.hint}</p><button class="primary" on:click={initialize}>{t('retry')}</button></section>
    {:else if config && snapshot}
      <UpdateBanner {lang} />
      {#if snapshot.recovered_corrupt}<div class="notice" role="status">{t('recovered')}</div>{/if}
      {#if wizard}<PrinterWizard {config} {snapshot} {editing} onconfig={update} onclose={() => { wizard = false; editing = null; view = 'status'; }} />
      {:else if view === 'settings'}<Preferences {config} onconfig={update} onedit={add} onadd={() => add()} />
      {:else}
        <div class="section-heading"><div><p class="eyebrow">{t('local')}</p><h1>{t('printers')}</h1></div><button on:click={() => add()}>{t('add')}</button></div>
        <div class="dashboard">
          <div class="printer-list" aria-label={t('printers')}>
            {#each config.printers.slice(printerPage, printerPage + 1) as printer (printer.id)}<PrinterCard {printer} snapshot={snapshot.printers.find(p => p.id === printer.id)} {lang} />{/each}
            {#if !config.printers.length}<div class="panel empty"><img src={logo} alt="" width="80" height="80" /><p>{t('intro')}</p><button class="primary" on:click={() => add()}>{t('add')}</button></div>{/if}
            {#if config.printers.length > 1}<div class="pagination"><button disabled={printerPage === 0} on:click={() => printerPage--}>{t('previous')}</button><small>{printerPage + 1} / {config.printers.length}</small><button disabled={printerPage >= config.printers.length - 1} on:click={() => printerPage++}>{t('pageNext')}</button></div>{/if}
          </div>
          <aside class="preview-column" aria-label={t('preview')}>
            <div class="preview-label"><h2>{t('preview')}</h2><p>{t('previewHint')}</p></div>
            <ActivityPreview activity={snapshot.activity} {lang} />
            <div class="discord-state" role="status"><span class="badge" class:online={snapshot.discord === 'connected'}><i></i>{t(discordKey)}</span>
              {#if snapshot.discord === 'no_app_id'}<p>{t('appIdHint')}</p><button on:click={() => view = 'settings'}>{t('settings')}</button>
              {:else if snapshot.discord === 'disconnected' || snapshot.discord === 'error'}<p>{t('discordWait')}</p>
              {:else if !snapshot.activity}<p>{t('noActivityHint')}</p>{/if}
            </div>
          </aside>
        </div>
        <footer><span class="signal"></span>{t('local')}{#if snapshot.discord !== 'connected' && snapshot.discord !== 'disabled'}<details class="discord-help"><summary>{t('discordReminder')}</summary><p>{t('discordHelp')}</p></details>{/if}</footer>
      {/if}
    {/if}
  </main>
</div>
