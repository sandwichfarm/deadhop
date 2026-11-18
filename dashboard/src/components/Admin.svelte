<script lang="ts">
  import type { PayApi } from '../lib/api';
  import type { Info, Invoice } from '../lib/schemas';
  import Invoices from './Invoices.svelte';
  import Maintenance from './Maintenance.svelte';
  import Overview from './Overview.svelte';
  import Settings from './Settings.svelte';

  type Tab = 'overview' | 'invoices' | 'settings' | 'maintenance';

  let {
    api,
    info,
    onCreated,
    dashboardRelays,
    onSettingsChanged,
  }: {
    api: PayApi;
    info: Info;
    onCreated: (invoice: Invoice) => void;
    dashboardRelays: string[];
    onSettingsChanged: () => void;
  } = $props();

  let tab = $state<Tab>('overview');
  const TABS: { id: Tab; label: string }[] = [
    { id: 'overview', label: 'Overview' },
    { id: 'invoices', label: 'Invoices' },
    { id: 'settings', label: 'Settings' },
    { id: 'maintenance', label: 'Maintenance' },
  ];
</script>

<nav class="tabs" aria-label="Operator">
  {#each TABS as t (t.id)}
    <button class={tab === t.id ? 'primary' : ''} aria-current={tab === t.id ? 'page' : undefined} onclick={() => (tab = t.id)}>{t.label}</button>
  {/each}
</nav>

{#if tab === 'overview'}
  <Overview {api} />
{:else if tab === 'invoices'}
  <Invoices {api} {info} {onCreated} />
{:else if tab === 'settings'}
  <Settings {api} {dashboardRelays} onChanged={onSettingsChanged} />
{:else}
  <Maintenance {api} />
{/if}
