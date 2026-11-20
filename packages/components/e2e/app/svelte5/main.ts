import { mount } from 'svelte';
import { PayButton, createDeadhop } from '@deadhop/components/svelte5';
import '@deadhop/components/styles.css';
import { config, events } from '../shared.ts';

const client = createDeadhop(await config());
mount(PayButton, { target: document.getElementById('app')!, props: { client, amount: 5000, memo: 'browser e2e (svelte5)', pollMs: 500, ...events } });
