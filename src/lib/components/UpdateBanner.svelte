<script lang="ts">
  import { onMount } from 'svelte';
  import { getUpdateState, installUpdate, onUpdate, type UpdateSnapshot } from '../api';
  import { translate } from '../i18n';
  import type { Lang } from '../api';

  export let lang: Lang;
  // Also permits deterministic server rendering of each banner state in tests.
  export let initial: UpdateSnapshot | null = null;
  let snapshot = initial;
  let visibleVersion: string | null = null;
  let dismissedVersion: string | null = null;
  let busy = false;
  let failure = false;
  $: t = (key: Parameters<typeof translate>[1]) => translate(lang, key);
  $: if (snapshot?.status.state === 'available') visibleVersion = snapshot.status.version;
  $: shown = snapshot && (
    (snapshot.status.state === 'available' && snapshot.status.version !== dismissedVersion) ||
    (snapshot.status.state === 'downloading' && (dismissedVersion === null || visibleVersion !== dismissedVersion)) ||
    (snapshot.status.state === 'ready' && (dismissedVersion === null || visibleVersion !== dismissedVersion)) ||
    (snapshot.status.state === 'error' && failure && visibleVersion !== dismissedVersion)
  );

  onMount(() => {
    let stopped = false;
    let received = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      unlisten = await onUpdate(value => { if (!stopped) { received = true; snapshot = value; } });
      if (stopped) { unlisten(); return; }
      const latest = await getUpdateState();
      if (!stopped && !received) snapshot = latest;
    })().catch(() => { /* app remains usable without update status */ });
    return () => { stopped = true; unlisten?.(); };
  });

  async function install() {
    if (busy) return;
    busy = true; failure = false;
    try { await installUpdate(); }
    catch { failure = true; }
    finally { busy = false; }
  }
</script>

{#if shown}
  <aside class="update-banner" role="status" aria-label={t('updateAvailable')}>
    <span class="update-dot" aria-hidden="true"></span>
    {#if snapshot?.status.state === 'available'}
      <span>{t('updateAvailable')} v{snapshot.status.version} {t('separator')}</span>
      <button class="update-action" on:click={install} disabled={busy}>{t('updateRestart')}</button>
    {:else if snapshot?.status.state === 'downloading'}
      <span>{t('updateDownloading')} {snapshot.status.percent}%</span>
      <progress value={snapshot.status.percent} max="100" aria-label={t('updateDownloading')}></progress>
    {:else if snapshot?.status.state === 'ready'}
      <span>{t('updateInstalling')}</span>
    {:else}
      <span>{t('updateFailed')}</span>
      <button class="update-action" on:click={install} disabled={busy}>{t('retry')}</button>
    {/if}
    {#if snapshot?.status.state === 'available' || snapshot?.status.state === 'error'}
      <button class="update-later" on:click={() => dismissedVersion = visibleVersion}>{t('updateLater')}</button>
    {/if}
  </aside>
{/if}

<style>
  .update-banner { display: flex; align-items: center; gap: 8px; min-height: 38px; padding: 5px 10px; margin-bottom: 12px; border: 1px solid #52dda055; border-radius: 9px; background: #52dda010; color: #d9f7e9; font-size: 12px; }
  .update-dot { flex: none; width: 6px; height: 6px; border-radius: 50%; background: var(--mint); }
  .update-action, .update-later { border: 0; background: transparent; padding: 4px; font-size: 12px; }
  .update-action { color: var(--mint); font-weight: 700; }
  .update-later { margin-left: auto; color: var(--muted); }
  progress { width: 90px; height: 5px; }
</style>
