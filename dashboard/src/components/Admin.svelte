<script lang="ts">
  import type { PayApi } from '../lib/api';
  import type { Info, Invoice } from '../lib/schemas';
  import Invoices from './Invoices.svelte';
  import Maintenance from './Maintenance.svelte';
  import Operators from './Operators.svelte';
  import Overview from './Overview.svelte';
  import PrivacyAudit from './PrivacyAudit.svelte';
  import Settings from './Settings.svelte';

  type Tab = 'overview' | 'privacy' | 'invoices' | 'settings' | 'operators' | 'maintenance';

  let {
    api,
    info,
    onCreated,
    dashboardRelays,
    serverPubkey,
    onSettingsChanged,
  }: {
    api: PayApi;
    info: Info;
    onCreated: (invoice: Invoice) => void;
    dashboardRelays: string[];
    serverPubkey: string;
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
      <button aria-current={tab === t.id || (tab === 'privacy' && t.id === 'overview') ? 'page' : undefined} onclick={() => (tab = t.id)}>{t.label}</button>
    {/each}
  </nav>
</div>

<main>

{#if tab === 'overview'}
  <Overview {api} {serverPubkey} onAudit={() => (tab = 'privacy')} onDrops={() => (tab = 'invoices')} />
{:else if tab === 'privacy'}
  <PrivacyAudit {api} onBack={() => (tab = 'overview')} />
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
