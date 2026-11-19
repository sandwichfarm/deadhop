import { createRoot } from 'react-dom/client';
import { PayButton, createCvmpay } from '@cvmpay/components/react';
import '@cvmpay/components/styles.css';
import { config, events } from '../shared.ts';

const client = createCvmpay(await config());
createRoot(document.getElementById('app')!).render(<PayButton client={client} amount={5000} memo="browser e2e (react)" pollMs={500} {...events} />);
