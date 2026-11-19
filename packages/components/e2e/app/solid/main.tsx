import { render } from 'solid-js/web';
import { PayButton, createCvmpay } from '@cvmpay/components/solid';
import '@cvmpay/components/styles.css';
import { config, events } from '../shared.ts';

const client = createCvmpay(await config());
render(() => <PayButton client={client} amount={5000} memo="browser e2e (solid)" pollMs={500} {...events} />, document.getElementById('app')!);
