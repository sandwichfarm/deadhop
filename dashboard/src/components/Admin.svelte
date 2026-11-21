<script lang="ts">
  import type { PayApi } from '../lib/api';
  import type { Info, Invoice } from '../lib/schemas';
  import Invoices from './Invoices.svelte';
  import Connectivity from './Connectivity.svelte';
  import Maintenance from './Maintenance.svelte';
  import Operators from './Operators.svelte';
  import Overview from './Overview.svelte';
  import PrivacyAudit from './PrivacyAudit.svelte';
  import Settings from './Settings.svelte';

  type Tab = 'overview' | 'privacy' | 'connectivity' | 'invoices' | 'settings' | 'operators' | 'maintenance';

  let {
    api,
    info,
    onCreated,
    dashboardRelays,
    serverPubkey,
    adminPubkey,
    onSettingsChanged,
  }: {
    api: PayApi;
    info: Info;
    onCreated: (invoice: Invoice) => void;
    dashboardRelays: string[];
    serverPubkey: string;
    adminPubkey: string;
    onSettingsChanged: () => void;
  } = $props();

  let tab = $state<Tab>('overview');
  const TABS: { id: Tab; label: string }[] = [
    { id: 'overview', label: 'Overview' },
    { id: 'invoices', label: 'Invoices' },
    { id: 'settings', label: 'Settings' },
    { id: 'operators', label: 'Operators' },
    { id: 'maintenance', label: 'Maintenance' },
  ];
</script>

<div class="tabbar">
  <nav class="tabs" aria-label="Operator">
    {#each TABS as t (t.id)}
      <button aria-current={tab === t.id || (['privacy', 'connectivity'].includes(tab) && t.id === 'overview') ? 'page' : undefined} onclick={() => (tab = t.id)}>{t.label}</button>
    {/each}
  </nav>
</div>

<main>

{#if tab === 'overview'}
  <Overview {api} {serverPubkey} {adminPubkey} onAudit={() => (tab = 'privacy')} onDrops={() => (tab = 'invoices')} onConnectivity={() => (tab = 'connectivity')} />
{:else if tab === 'privacy'}
  <PrivacyAudit {api} onBack={() => (tab = 'overview')} />
{:else if tab === 'connectivity'}
  <Connectivity {api} onBack={() => (tab = 'overview')} />
{:else if tab === 'invoices'}
  <Invoices {api} {info} {onCreated} />
{:else if tab === 'settings'}
  <Settings {api} {dashboardRelays} onChanged={onSettingsChanged} />
{:else if tab === 'operators'}
  <Operators {api} />
{:else}
  <Maintenance {api} />
{/if}
</main>
