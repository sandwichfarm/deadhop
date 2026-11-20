import { render } from 'solid-js/web';
import { PayButton, createDeadhop } from '@deadhop/solid';
import '@deadhop/solid/styles.css';
import { config, events } from '../shared.ts';

const client = createDeadhop(await config());
render(() => <PayButton client={client} amount={5000} memo="browser e2e (solid)" pollMs={500} {...events} />, document.getElementById('app')!);
