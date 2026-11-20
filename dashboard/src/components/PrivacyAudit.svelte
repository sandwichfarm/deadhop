<script lang="ts">
  import type { PayApi } from '../lib/api';
  import type { PrivacyAudit, PrivacyCheck } from '../lib/schemas';
  import { formatTime } from '../lib/format';

  let { api, onBack }: { api: PayApi; onBack: () => void } = $props();

  let audit = $state<PrivacyAudit | null>(null);
  let error = $state('');
  let running = $state(false);

  const AREAS: { id: PrivacyCheck['area']; title: string }[] = [
    { id: 'network', title: 'Network location' },
    { id: 'ports', title: 'Open ports' },
    { id: 'exposure', title: 'Exposure' },
  ];
  const RESULT: Record<PrivacyCheck['result'], string> = {
    pass: 'pass',
    to_fix: 'to fix',
    fail: 'fail',
    note: 'note',
    not_checked: 'not checked',
  };

  async function run() {
    running = true;
    error = '';
    try {
      audit = await api.getPrivacyAudit();
    } catch (e) {
      error = (e as Error).message;
    }
    running = false;
  }
  run();

  /** Ring length out of 100 for the score. */
  const RING = 2 * Math.PI * 52;
  function tone(score: number): string {
    return score >= 90 ? 'ok' : score >= 60 ? 'warn' : 'bad';
  }
  function attention(n: number): string {
    return n === 0 ? 'Nothing needs attention' : `${n} need${n === 1 ? 's' : ''} attention`;
  }
  function count(a: PrivacyAudit, result: PrivacyCheck['result']): number {
    return a.checks.filter((c) => c.result === result).length;
  }
</script>

<button class="link" onclick={onBack}>← Overview</button>

{#if error}
  <p class="error" role="alert">{error}</p>
{/if}

{#if audit}
  <section class="card audit-head" aria-label="Privacy score">
    <svg class={`ring ${tone(audit.score)}`} viewBox="0 0 120 120" role="img" aria-label={`Privacy score ${audit.score} out of 100`}>
      <circle class="track" cx="60" cy="60" r="52"></circle>
      <circle class="value" cx="60" cy="60" r="52" stroke-dasharray={`${(audit.score / 100) * RING} ${RING}`} transform="rotate(-90 60 60)"></circle>
      <text x="60" y="68" text-anchor="middle">{audit.score}</text>
    </svg>
    <div class="audit-summary">
      <p class="muted">{`Privacy audit · ${formatTime(audit.ran_at)} · ${audit.checked} of ${audit.total} checked`}</p>
      <h2>{attention(count(audit, 'fail') + count(audit, 'to_fix'))}</h2>
      <p class="stamps">
        <span class="badge pass">{`${count(audit, 'pass')} pass`}</span>
        <span class="badge to_fix">{`${count(audit, 'to_fix')} to fix`}</span>
        <span class="badge fail">{`${count(audit, 'fail')} fail`}</span>
        <span class="badge not_checked">{`${count(audit, 'not_checked')} not checked`}</span>
      </p>
      <p class="muted">The score counts only checks that ran. Checks this service cannot make say why, and how to make them yourself.</p>
    </div>
    <button class="primary" onclick={run} disabled={running}>{running ? 'Running…' : 'Run again'}</button>
  </section>

  {#each AREAS as area (area.id)}
    {#if audit.checks.some((c) => c.area === area.id)}
      <section class="card">
        <h2>{area.title}</h2>
        {#each audit.checks.filter((c) => c.area === area.id) as c (c.id)}
          <details class="check" open={c.result === 'fail' || c.result === 'to_fix'}>
            <summary>
              <span class={`badge ${c.result}`}>{RESULT[c.result]}</span>
              <span class="check-title">{c.title}</span>
              {#if c.points > 0}<span class="mono muted">{`−${c.points}`}</span>{/if}
            </summary>
            <p>{c.summary}</p>
            {#if c.evidence.length > 0}
              <pre class="evidence">{c.evidence.join('\n')}</pre>
            {/if}
            {#if c.fix}
              <p class="fix"><strong>Fix:</strong> {`${c.fix}`}</p>
            {/if}
          </details>
        {/each}
      </section>
    {/if}
  {/each}
{/if}
