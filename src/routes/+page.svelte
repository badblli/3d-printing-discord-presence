<script lang="ts">
  import mascot from '$lib/assets/mascot.png';
  import { onMount } from 'svelte';
  import { getConfig, getSnapshot, onSnapshot, type ConfigView, type Snapshot, type PrinterConfigView } from '$lib/api';
  import { translate, type Key } from '$lib/i18n';
  import { errorMessage } from '$lib/errors';
  import PrinterWizard from '$lib/components/PrinterWizard.svelte';
  import PrinterCard from '$lib/components/PrinterCard.svelte';
  import Preferences from '$lib/components/Preferences.svelte';
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
    finally { if (!disposed) loading = false; }
  }
  onMount(() => { void initialize(); return () => { disposed = true; generation++; unsubscribe?.(); }; });
  function update(value: ConfigView) { config = value; }
  function add(printer: PrinterConfigView | null = null) { editing = printer; wizard = true; }
</script>

<svelte:head><title>{t('app')}</title></svelte:head>
<div class="app-shell">
  <header>
    <div class="brand"><img src={mascot} alt="" width="44" height="44" /><div><strong>{t('app')}</strong><small>{t('tagline')}</small></div></div>
    {#if config && !wizard}<nav aria-label={t('app')}><button class:current={view === 'status'} aria-current={view === 'status' ? 'page' : undefined} on:click={() => view = 'status'}>{t('status')}</button><button class:current={view === 'settings'} aria-current={view === 'settings' ? 'page' : undefined} on:click={() => view = 'settings'}>{t('settings')}</button></nav>{/if}
  </header>
  <main>
    {#if loading}<section class="panel empty" role="status"><img src={mascot} alt="" width="96" height="96" /><p>{t('loading')}</p></section>
    {:else if failure}<section class="panel empty" role="alert"><h1>{problem.title}</h1><p>{problem.hint}</p><button class="primary" on:click={initialize}>{t('retry')}</button></section>
    {:else if config && snapshot}
      {#if snapshot.recovered_corrupt}<div class="notice" role="status">{t('recovered')}</div>{/if}
      {#if wizard}<PrinterWizard {config} {snapshot} {editing} onconfig={update} onclose={() => { wizard = false; editing = null; view = 'status'; }} />
      {:else if view === 'settings'}<Preferences {config} onconfig={update} onedit={add} onadd={() => add()} />
      {:else}
        <div class="section-heading"><div><p class="eyebrow">{t('local')}</p><h1>{t('printers')}</h1></div><button on:click={() => add()}>{t('add')}</button></div>
        <div class="dashboard">
          <div class="printer-list" aria-label={t('printers')}>
            {#each config.printers as printer (printer.id)}<PrinterCard {printer} snapshot={snapshot.printers.find(p => p.id === printer.id)} {lang} />{/each}
            {#if !config.printers.length}<div class="panel empty"><img src={mascot} alt="" width="80" height="80" /><p>{t('intro')}</p><button class="primary" on:click={() => add()}>{t('add')}</button></div>{/if}
          </div>
          <aside class="preview-column" aria-label={t('preview')}>
            <div class="preview-label"><h2>{t('preview')}</h2><p>{t('previewHint')}</p></div>
            <div class="discord-card">
              <div class="profile-banner"></div><div class="profile-avatar"><img src={mascot} alt="" width="62" height="62" /><i></i></div>
              <div class="activity"><p class="eyebrow">{t('playing')}</p><div class="activity-row"><img src={mascot} alt={snapshot.activity?.assets.large_text ?? ''} width="60" height="60" /><div><strong>{t('app')}</strong>{#if snapshot.activity}<p>{snapshot.activity.details}</p><p>{snapshot.activity.state}</p>{:else}<p>{t('noActivity')}</p>{/if}</div></div></div>
            </div>
            <div class="discord-state" role="status"><span class="badge" class:online={snapshot.discord === 'connected'}><i></i>{t(discordKey)}</span>
              {#if snapshot.discord === 'no_app_id'}<p>{t('appIdHint')}</p><button on:click={() => view = 'settings'}>{t('settings')}</button>
              {:else if snapshot.discord === 'disconnected' || snapshot.discord === 'error'}<p>{t('discordWait')}</p>
              {:else if !snapshot.activity}<p>{t('noActivityHint')}</p>{/if}
            </div>
          </aside>
        </div>
        <footer><span class="signal"></span>{t('local')}<details class="discord-help"><summary>{t('discordReminder')}</summary><p>{t('discordHelp')}</p></details></footer>
      {/if}
    {/if}
  </main>
</div>
