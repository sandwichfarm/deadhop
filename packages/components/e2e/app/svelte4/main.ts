// The Svelte 4 components, compiled here by Svelte 5 in legacy mode.
import { mount } from 'svelte';
import { PayButton, createDeadhop } from '@deadhop/components/svelte4';
import '@deadhop/components/styles.css';
import { config, events } from '../shared.ts';

const client = createDeadhop(await config());
mount(PayButton as never, { target: document.getElementById('app')!, props: { client, amount: 5000, memo: 'browser e2e (svelte4)', pollMs: 500, ...events } });
