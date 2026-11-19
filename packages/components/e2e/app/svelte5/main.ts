import { mount } from 'svelte';
import { PayButton, createCvmpay } from '@cvmpay/components/svelte5';
import '@cvmpay/components/styles.css';
import { config, events } from '../shared.ts';

const client = createCvmpay(await config());
mount(PayButton, { target: document.getElementById('app')!, props: { client, amount: 5000, memo: 'browser e2e (svelte5)', pollMs: 500, ...events } });
