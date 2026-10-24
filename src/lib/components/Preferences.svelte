<script lang="ts">
  import { saveConfig, setAutostart, deletePrinter, type ConfigView, type PrinterConfigView } from '../api';
  import { translate, type Key } from '../i18n';
  import { validAppId } from '../forms';
  import { errorMessage } from '../errors';
  export let config: ConfigView;
  export let onconfig: (value: ConfigView) => void;
  export let onedit: (printer: PrinterConfigView) => void;
  export let onadd: () => void;
  let draft = { ...config, discord: { ...config.discord } };
  let appId = config.discord.app_id_override ?? '';
  let busy = false;
  let failure: unknown = null;
  let saved = false;
  let invalidId = false;
  let removing: string | null = null;
  $: lang = draft.lang;
  $: t = (key: Key) => translate(lang, key);
  $: problem = errorMessage(failure, lang);
  async function persist() {
    invalidId = !validAppId(appId);
    if (invalidId) return;
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
    try { const updated = await deletePrinter(id); config = updated; onconfig(updated); removing = null; }
    catch (error) { failure = error; }
    finally { busy = false; }
  }
</script>

<section class="panel preferences" aria-labelledby="preferences-title">
  <h1 id="preferences-title">{t('preferences')}</h1>
  {#if failure}<div class="notice danger" role="alert"><strong>{problem.title}</strong><p>{problem.hint}</p></div>{/if}
  <form on:submit|preventDefault={persist}>
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
    <div class="actions"><span role="status">{saved ? t('saved') : ''}</span><button class="primary" disabled={busy}>{t(busy ? 'saving' : 'save')}</button></div>
  </form>
  <div class="section-heading"><h2>{t('printers')}</h2><button disabled={busy} on:click={onadd}>{t('add')}</button></div>
  {#each config.printers as printer (printer.id)}
    <div class="manage-printer"><strong>{printer.name}</strong><div><button disabled={busy || printer.connection.kind !== 'bambu'} on:click={() => onedit(printer)}>{t('edit')}</button><button class="text-danger" disabled={busy} on:click={() => removing = printer.id}>{t('remove')}</button></div></div>
    {#if removing === printer.id}<div class="notice danger" role="alert"><p>{t('deleteConfirm')}</p><div class="actions"><button disabled={busy} on:click={() => removing = null}>{t('cancel')}</button><button disabled={busy} on:click={() => remove(printer.id)}>{t('confirmRemove')}</button></div></div>{/if}
  {/each}
</section>
