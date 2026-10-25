<script lang="ts">
  import { tick } from 'svelte';
  import type { DiscordConfig, Lang, PrintStatus } from '../api';
  import { translate, type Key } from '../i18n';
  import { placeholders, defaultDetails, defaultState, renderTemplate, validButtons, validTemplate } from '../card';
  import ActivityPreview from './ActivityPreview.svelte';
  import logo from '../assets/logo.png';
  export let discord: DiscordConfig;
  export let lang: Lang;
  export let busy = false;
  let focused: 'details_template' | 'state_template' = 'details_template';
  let detailsInput: HTMLInputElement;
  let stateInput: HTMLInputElement;
  const sample: PrintStatus = { kind: 'bambu', state: 'printing', progress: 42, remaining_secs: 4800,
    file_name: 'example.gcode.3mf', printer_model: 'Bambu Lab A1', layer: 12, total_layers: 200, nozzle_temp: 215, bed_temp: 60, observed_at_ms: 0 };
  $: t = (key: Key) => translate(lang, key);
  $: details = discord.details_template ?? defaultDetails;
  $: state = discord.state_template ?? defaultState;
  $: preview = {
    details: renderTemplate(details, sample, lang, discord.show_file_name),
    state: renderTemplate(state, sample, lang, discord.show_file_name),
    assets: { large_image: logo, large_text: sample.printer_model!, small_image: discord.show_status_badge ? 'https://raw.githubusercontent.com/badblli/3d-printing-discord-presence/main/assets/discord/status-printing.png' : undefined, small_text: t('printing') },
    buttons: discord.buttons.filter(button => validButtons([button])),
  };
  async function insert(key: string) {
    const input = focused === 'details_template' ? detailsInput : stateInput;
    const text = discord[focused] ?? (focused === 'details_template' ? defaultDetails : defaultState);
    const start = input.selectionStart ?? text.length;
    const end = input.selectionEnd ?? start;
    const token = `{${key}}`;
    discord[focused] = text.slice(0, start) + token + text.slice(end);
    await tick(); input.focus(); input.setSelectionRange(start + token.length, start + token.length);
  }
</script>

<div class="card-editor">
  <div class="template-editor">
    <div class="field"><label for="details-template">{t('detailsTemplate')}</label><input id="details-template" bind:this={detailsInput} value={details} on:input={event => discord.details_template = event.currentTarget.value} on:focus={() => focused = 'details_template'} aria-invalid={!validTemplate(details)} aria-describedby="template-help" disabled={busy} /></div>
    <div class="field"><label for="state-template">{t('stateTemplate')}</label><input id="state-template" bind:this={stateInput} value={state} on:input={event => discord.state_template = event.currentTarget.value} on:focus={() => focused = 'state_template'} aria-invalid={!validTemplate(state)} aria-describedby="template-help" disabled={busy} /></div>
    <small id="template-help">{t('templateHint')}</small>
    <div class="placeholder-chips">{#each placeholders as key}<button type="button" disabled={busy} on:click={() => insert(key)}>{`{${key}}`}</button>{/each}</div>
    {#if !validTemplate(details) || !validTemplate(state)}<small class="invalid" role="alert">{t('invalidTemplate')}</small>{/if}
    <div class="card-options"><button type="button" disabled={busy} on:click={() => { discord.details_template = null; discord.state_template = null; }}>{t('resetTemplates')}</button><label>{t('statusBadge')} <input type="checkbox" bind:checked={discord.show_status_badge} disabled={busy} /></label></div>
    <div class="links-heading"><small>{t('buttonsHint')}</small><button type="button" disabled={busy || discord.buttons.length >= 2} on:click={() => discord.buttons = [...discord.buttons, { label: '', url: '' }]}>{t('addButton')}</button></div>
    {#each discord.buttons as button, i}
      <div class="button-editor">
        <input aria-label={`${t('buttonLabel')} ${i + 1}`} placeholder={t('buttonLabel')} bind:value={button.label} aria-invalid={!validButtons([button])} disabled={busy} />
        <input aria-label={`${t('buttonUrl')} ${i + 1}`} placeholder="https://example.com" bind:value={button.url} aria-invalid={!validButtons([button])} disabled={busy} spellcheck="false" />
        <button type="button" aria-label={`${t('removeButton')} ${i + 1}`} disabled={busy} on:click={() => discord.buttons = discord.buttons.filter((_, index) => index !== i)}>×</button>
      </div>
    {/each}
    {#if !validButtons(discord.buttons)}<small class="invalid" role="alert">{t('invalidButtons')}</small>{/if}
  </div>
  <aside class="editor-preview"><p class="eyebrow">{t('preview')}</p><ActivityPreview activity={preview} {lang} /><p class="sample-note">{t('samplePreview')}</p></aside>
</div>
