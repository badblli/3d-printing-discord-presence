<script lang="ts">
  import mascot from '../assets/mascot.png';
  import { onDestroy, tick } from 'svelte';
  import { saveConfig, importLegacy, getSnapshot, type ConfigView, type Snapshot, type PrinterConfigView, type Lang } from '../api';
  import { translate, type Key } from '../i18n';
  import { validatePrinter, type PrinterForm, type FormErrors } from '../forms';
  import { errorMessage } from '../errors';

  export let config: ConfigView;
  export let snapshot: Snapshot;
  export let editing: PrinterConfigView | null = null;
  export let onconfig: (config: ConfigView) => void;
  export let onclose: () => void;
  $: lang = config.lang;
  $: t = (key: Key) => translate(lang, key);
  let step = editing ? 1 : 0;
  let form: PrinterForm = {
    name: editing?.name ?? '',
    host: editing?.connection.kind === 'bambu' ? editing.connection.host : '',
    serial: editing?.connection.kind === 'bambu' ? editing.connection.serial : '',
    model: editing?.connection.kind === 'bambu' ? editing.connection.model ?? '' : '', secret: '',
  };
  let errors: FormErrors = {};
  let busy = false;
  let failure: unknown = null;
  let printerId = editing?.id ?? '';
  let timedOut = false;
  let timer: ReturnType<typeof setTimeout>;
  $: tested = snapshot.printers.find(p => p.id === printerId);
  $: connection = tested?.connection;
  $: problem = errorMessage(failure, lang);
  onDestroy(() => { clearTimeout(timer); form.secret = ''; });

  async function persist(event: SubmitEvent) {
    const element = event.currentTarget as HTMLFormElement;
    errors = validatePrinter(form, editing?.has_secret);
    if (Object.keys(errors).length) {
      await tick();
      element.querySelector<HTMLInputElement>('[aria-invalid="true"]')?.focus();
      return;
    }
    busy = true; failure = null;
    try {
      printerId ||= crypto.randomUUID();
      const printer = { id: printerId, name: form.name.trim(), enabled: true,
        connection: { kind: 'bambu' as const, host: form.host.trim(), serial: form.serial.trim(), model: form.model.trim() || null } };
      const updated = await saveConfig({ ...config, printers: [...config.printers.filter(p => p.id !== printerId), printer] },
        form.secret ? [{ id: printerId, secret: form.secret }] : []);
      form.secret = '';
      editing = updated.printers.find(p => p.id === printerId) ?? null;
      onconfig(updated);
      step = 2; timedOut = false;
      timer = setTimeout(() => { timedOut = true; }, 20000);
      snapshot = await getSnapshot();
    } catch (error) { failure = error; }
    finally { busy = false; }
  }

  async function migrate() {
    busy = true; failure = null;
    try {
      const updated = await importLegacy();
      onconfig(updated);
      snapshot = await getSnapshot();
      step = updated.printers.length ? 3 : 1;
    } catch (error) { failure = error; }
    finally { busy = false; }
  }
  function changeLanguage(event: Event) {
    onconfig({ ...config, lang: (event.target as HTMLSelectElement).value as Lang });
  }
</script>

<section class="wizard panel" aria-labelledby="wizard-title">
  <div class="wizard-top">
    <div class="steps" aria-label={t('step')}>
      {#each [0, 1, 2, 3] as n}<span class:active={step >= n} aria-current={step === n ? 'step' : undefined}>{n + 1}</span>{/each}
    </div>
    <label class="language"><span class="sr-only">{t('language')}</span><select value={lang} on:change={changeLanguage} disabled={busy}><option value="tr">{t('tr')}</option><option value="en">{t('en')}</option></select></label>
  </div>
  {#if failure}<div class="notice danger" role="alert"><strong>{problem.title}</strong><p>{problem.hint}</p></div>{/if}

  {#if step === 0}
    <div class="welcome"><img src={mascot} alt="" width="96" height="96" /><div><p class="eyebrow">{t('app')}</p><h1 id="wizard-title">{t('welcome')}</h1><p>{t('intro')}</p></div></div>
    {#if snapshot.legacy_available}<div class="notice"><p>{t('legacy')}</p><button disabled={busy} on:click={migrate}>{t('import')}</button></div>{/if}
    <h2>{t('choose')}</h2><p>{t('chooseHint')}</p>
    <div class="type-grid">
      {#each ['bambu', 'moonraker', 'octo_print', 'prusa_link'] as kind}
        <button class:selected={kind === 'bambu'} disabled={kind !== 'bambu' || busy} on:click={() => step = 1}>
          <span>{t(kind as Key)}</span><small>{kind === 'bambu' ? t('next') : t('soon')}</small>
        </button>
      {/each}
    </div>
  {:else if step === 1}
    <h1 id="wizard-title">{t('details')}</h1><p>{t('detailsHint')}</p>
    <form on:submit|preventDefault={persist} novalidate>
      <fieldset class="fields" disabled={busy} aria-label={t('details')}>
        <div class="field full"><label for="printer-name">{t('name')}</label><input id="printer-name" bind:value={form.name} aria-invalid={!!errors.name} aria-describedby={errors.name ? 'name-error' : undefined} maxlength="80" />{#if errors.name}<small class="invalid" id="name-error">{t(errors.name)}</small>{/if}</div>
        {#each ['host', 'serial', 'secret', 'model'] as field}
          <div class="field">
            <label for={'printer-' + field}>{t(field as Key)}</label>
            {#if field === 'secret'}
              <input id="printer-secret" type="password" autocomplete="new-password" spellcheck="false" bind:value={form.secret} placeholder={editing?.has_secret ? t('stored') : ''} aria-invalid={!!errors.secret} aria-describedby={errors.secret ? 'secret-error' : undefined} />
            {:else}
              <input id={'printer-' + field} bind:value={form[field as keyof PrinterForm]} spellcheck="false" autocapitalize="off" placeholder={field === 'model' ? t('automatic') : ''} aria-invalid={!!errors[field as keyof PrinterForm]} aria-describedby={errors[field as keyof PrinterForm] ? field + '-error' : undefined} />
            {/if}
            {#if errors[field as keyof PrinterForm]}<small class="invalid" id={field + '-error'}>{t(errors[field as keyof PrinterForm]!)}</small>{/if}
            <details class="help"><summary>{t('find')}</summary><p>{t((field + 'Help') as Key)}</p></details>
          </div>
        {/each}
      </fieldset>
      <div class="actions"><button type="button" disabled={busy} on:click={() => editing ? onclose() : step = 0}>{t(editing ? 'cancel' : 'back')}</button><button class="primary" disabled={busy}>{t(busy ? 'saving' : 'test')}</button></div>
    </form>
  {:else if step === 2}
    <h1 id="wizard-title">{t('testTitle')}</h1><p>{t('testHint')}</p>
    <div class="test-result" aria-live="polite">
      <img src={mascot} alt="" width="88" height="88" />
      {#if connection?.state === 'connected'}<h2 class="mint">{t('success')}</h2>
      {:else if connection?.state === 'error'}{@const message = errorMessage(connection.error, lang)}<h2>{message.title}</h2><p>{message.hint}</p>
      {:else}<h2>{t(timedOut ? 'testPending' : 'testing')}</h2>{/if}
    </div>
    <div class="actions"><button on:click={() => { clearTimeout(timer); step = 1; }}>{t('edit')}</button><button class="primary" on:click={() => step = 3}>{t(connection?.state === 'connected' ? 'next' : 'continueWaiting')}</button></div>
  {:else}
    <div class="test-result"><img src={mascot} alt="" width="96" height="96" /><h1 id="wizard-title">{t('discordReminder')}</h1><p>{t('discordHelp')}</p><p class="mint">{t('discordReady')}</p></div>
    <div class="actions"><button class="primary" on:click={onclose}>{t('finish')}</button></div>
  {/if}
  {#if step === 0 && config.printers.length}<div class="actions"><button disabled={busy} on:click={onclose}>{t('cancel')}</button></div>{/if}
</section>

