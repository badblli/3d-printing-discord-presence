<script lang="ts">
  import { onMount } from 'svelte';
  import type { Activity, Lang } from '../api';
  import { translate } from '../i18n';
  import { countdown } from '../card';
  import logo from '../assets/logo.png';
  export let activity: Activity | null;
  export let lang: Lang;
  let now = Date.now() / 1000;
  let failedLarge = '';
  let failedSmall = '';
  onMount(() => { const timer = setInterval(() => now = Date.now() / 1000, 1000); return () => clearInterval(timer); });
  $: t = (key: Parameters<typeof translate>[1]) => translate(lang, key);
</script>

<div class="discord-card">
  <div class="profile-banner"></div>
  <div class="profile-avatar" aria-hidden="true"><svg width="38" height="38" viewBox="0 0 32 32" fill="currentColor"><circle cx="16" cy="11" r="6"/><path d="M5 30v-4a11 11 0 0 1 22 0v4Z"/></svg><i></i></div>
  <div class="activity"><p class="eyebrow">{t('playing')}</p>
    <div class="activity-row">
      <div class="activity-art">
        <img src={activity && failedLarge !== activity.assets.large_image ? activity.assets.large_image : logo} alt={activity?.assets.large_text ?? ''} width="64" height="64" on:error={() => failedLarge = activity?.assets.large_image ?? ''} />
        {#if activity?.assets.small_image && failedSmall !== activity.assets.small_image}<img class="status-art" src={activity.assets.small_image} alt={activity.assets.small_text ?? ''} title={activity.assets.small_text} width="22" height="22" on:error={() => failedSmall = activity?.assets.small_image ?? ''} />{/if}
      </div>
      <div><strong>{t('discordApp')}</strong>{#if activity}<p>{activity.details}</p><p>{activity.state}</p>{#if activity.timestamps}<p class="countdown">{countdown(activity.timestamps.end, now)} {t('remaining')}</p>{/if}{:else}<p>{t('noActivity')}</p>{/if}</div>
    </div>
    {#if activity?.buttons?.length}<div class="preview-buttons">{#each activity.buttons as button}<div>{button.label}</div>{/each}</div>{/if}
  </div>
</div>
