<script lang="ts">
  import { saveConfig, setAutostart, deletePrinter, getUpdateState, checkForUpdates, onUpdate, type UpdateSnapshot, type ConfigView, type PrinterConfigView } from '../api';
  import { onMount } from 'svelte';
  import { translate, type Key } from '../i18n';
  import { validAppId } from '../forms';
  import { validButtons, validTemplate } from '../card';
  import CardSettings from './CardSettings.svelte';
  import { errorMessage } from '../errors';
  export let config: ConfigView;
  export let onconfig: (value: ConfigView) => void;
  export let onedit: (printer: PrinterConfigView) => void;
  export let onadd: () => void;
  export let initialUpdate: UpdateSnapshot | null = null;
  export let initialTab: 'general' | 'card' | 'printers' | 'updates' = 'general';
  let updateState = initialUpdate;
  let updateChecking = false;
  let updateFailure = false;
  onMount(() => {
    let stopped = false;
    let received = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      unlisten = await onUpdate(value => { if (!stopped) { received = true; updateState = value; } });
      if (stopped) { unlisten(); return; }
      const latest = await getUpdateState();
      if (!stopped && !received) updateState = latest;
    })().catch(() => { if (!stopped) updateFailure = true; });
    return () => { stopped = true; unlisten?.(); };
  });
  async function checkUpdates() {
    updateChecking = true; updateFailure = false;
    try { updateState = await checkForUpdates(); }
    catch { updateFailure = true; }
    finally { updateChecking = false; }
  }
  let draft = { ...config, discord: { ...config.discord, buttons: config.discord.buttons.map(button => ({ ...button })) } };
  let appId = config.discord.app_id_override ?? '';
  let busy = false;
  let failure: unknown = null;
  let saved = false;
  let invalidId = false;
  let tab: 'general' | 'card' | 'printers' | 'updates' = initialTab;
  let printerPage = 0;
  $: cardValid = validTemplate(draft.discord.details_template ?? '') && validTemplate(draft.discord.state_template ?? '') && validButtons(draft.discord.buttons);
  let removing: string | null = null;
  $: lang = draft.lang;
  $: t = (key: Key) => translate(lang, key);
  $: problem = errorMessage(failure, lang);
  async function persist() {
    invalidId = !validAppId(appId);
    if (invalidId) { tab = 'general'; return; }
    if (!cardValid) { tab = 'card'; return; }
    busy = true; saved = false; failure = null;
    try {
      // Apply OS startup separately, then use its returned authoritative state.
      if (draft.autostart !== config.autostart) {
        const updated = await setAutostart(draft.autostart);
        config = updated; onconfig(updated);
      }
      const updated = await saveConfig({ ...draft, printers: config.printers, discord: { ...draft.discord, app_id_override: appId.trim() || null } });
      config = updated; onconfig(updated); saved = true;
    } catch (error) { failure = error; }
    finally { busy = false; }
  }
  async function remove(id: string) {
    busy = true; failure = null;
    try { const updated = await deletePrinter(id); config = updated; onconfig(updated); removing = null; printerPage = Math.min(printerPage, Math.max(0, updated.printers.length - 1)); }
    catch (error) { failure = error; }
    finally { busy = false; }
  }
</script>

<section class="panel preferences" aria-labelledby="preferences-title">
  <div class="preferences-heading"><h1 id="preferences-title">{t('preferences')}</h1><div class="settings-tabs" aria-label={t('settings')}><button class:current={tab === 'general'} aria-pressed={tab === 'general'} on:click={() => tab = 'general'}>{t('general')}</button><button class:current={tab === 'card'} aria-pressed={tab === 'card'} on:click={() => tab = 'card'}>{t('cardSettings')}</button><button class:current={tab === 'printers'} aria-pressed={tab === 'printers'} on:click={() => tab = 'printers'}>{t('printers')}</button><button class:current={tab === 'updates'} aria-pressed={tab === 'updates'} on:click={() => tab = 'updates'}>{t('updates')}</button></div></div>
  {#if failure}<div class="notice danger" role="alert"><strong>{problem.title}</strong><p>{problem.hint}</p></div>{/if}
  {#if tab === 'general' || tab === 'card'}
  <form class:general-form={tab === 'general'} on:submit|preventDefault={persist} on:input={() => saved = false} on:change={() => saved = false}>
    {#if tab === 'card'}
      <CardSettings bind:discord={draft.discord} {lang} {busy} />
    {:else}
    {#each ['discordEnabled', 'showFile', 'autostart'] as key}
      <label class="toggle-row">
        <span><strong>{t(key as Key)}</strong><small>{t((key + 'Hint') as Key)}</small></span>
        {#if key === 'autostart'}<input type="checkbox" bind:checked={draft.autostart} disabled={busy} />
        {:else if key === 'showFile'}<input type="checkbox" bind:checked={draft.discord.show_file_name} disabled={busy} />
        {:else}<input type="checkbox" bind:checked={draft.discord.enabled} disabled={busy} />{/if}
      </label>
    {/each}
    <label class="toggle-row"><strong>{t('language')}</strong><select bind:value={draft.lang} disabled={busy}><option value="tr">{t('tr')}</option><option value="en">{t('en')}</option></select></label>
    <details class="advanced"><summary>{t('advanced')}</summary><div class="field"><label for="app-id">{t('appId')}</label><input id="app-id" bind:value={appId} inputmode="numeric" aria-invalid={invalidId} aria-describedby="app-id-help" disabled={busy} /><small id="app-id-help">{t('appIdHelp')}</small>{#if invalidId}<small class="invalid" role="alert">{t('invalidAppId')}</small>{/if}</div></details>
    {/if}
    <div class="actions"><span role="status">{saved ? t('saved') : ''}</span><button class="primary" disabled={busy || !cardValid}>{t(busy ? 'saving' : 'save')}</button></div>
  </form>
  {/if}
  {#if tab === 'updates'}
    <div class="update-settings">
      <div class="toggle-row"><strong>{t('currentVersion')}</strong><span>v{updateState?.current_version ?? '—'}</span></div>
      <div class="toggle-row"><strong>{t('lastUpdateCheck')}</strong><span>{updateState?.last_checked_ms ? new Date(updateState.last_checked_ms).toLocaleString(lang === 'tr' ? 'tr-TR' : 'en-US') : t('neverChecked')}</span></div>
      <div class="actions"><span role="status">{updateFailure || updateState?.status.state === 'error' ? t('updateCheckFailed') : updateState?.status.state === 'available' ? `${t('updateAvailable')} v${updateState.status.version}` : updateState?.status.state === 'idle' && updateState.last_checked_ms ? t('upToDate') : ''}</span><button class="primary" disabled={updateChecking || updateState?.status.state === 'checking' || updateState?.status.state === 'downloading'} on:click={checkUpdates}>{updateChecking || updateState?.status.state === 'checking' ? t('updateChecking') : t('checkNow')}</button></div>
    </div>
  {/if}
  {#if tab === 'printers'}
  <div class="section-heading"><h2>{t('printers')}</h2><button disabled={busy} on:click={onadd}>{t('add')}</button></div>
  {#each config.printers.slice(printerPage, printerPage + 1) as printer (printer.id)}
    <div class="manage-printer"><strong>{printer.name}</strong><div><button disabled={busy || printer.connection.kind !== 'bambu'} on:click={() => onedit(printer)}>{t('edit')}</button><button class="text-danger" disabled={busy} on:click={() => removing = printer.id}>{t('remove')}</button></div></div>
    {#if removing === printer.id}<div class="notice danger" role="alert"><p>{t('deleteConfirm')}</p><div class="actions"><button disabled={busy} on:click={() => removing = null}>{t('cancel')}</button><button disabled={busy} on:click={() => remove(printer.id)}>{t('confirmRemove')}</button></div></div>{/if}
  {/each}
  {#if config.printers.length > 1}<div class="pagination"><button disabled={busy || printerPage === 0} on:click={() => { printerPage--; removing = null; }}>{t('previous')}</button><small>{printerPage + 1} / {config.printers.length}</small><button disabled={busy || printerPage >= config.printers.length - 1} on:click={() => { printerPage++; removing = null; }}>{t('pageNext')}</button></div>{/if}
  {/if}
</section>
