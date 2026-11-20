import { createRoot } from 'react-dom/client';
import { PayButton, createDeadhop } from '@deadhop/react';
import '@deadhop/react/styles.css';
import { config, events } from '../shared.ts';

const client = createDeadhop(await config());
createRoot(document.getElementById('app')!).render(<PayButton client={client} amount={5000} memo="browser e2e (react)" pollMs={500} {...events} />);
