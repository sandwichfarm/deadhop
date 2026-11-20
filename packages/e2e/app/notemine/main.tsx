// PayButton with notemine doing the proof of work. The page records which
// miner produced the work, so the test can tell notemine from the fallback.
import { createRoot } from 'react-dom/client';
import { mine, type Miner } from '@deadhop/client';
import { notemineMiner } from '@deadhop/client/notemine';
import { PayButton, createDeadhop } from '@deadhop/react';
import '@deadhop/react/styles.css';
import { config, events } from '../shared.ts';

const mark = (name: string) => document.body.setAttribute(`data-${name}`, String(Number(document.body.getAttribute(`data-${name}`) ?? 0) + 1));
const fallback: Miner = (...args) => {
  mark('fallback');
  return mine(...args);
};
const notemine = notemineMiner({ fallback });
const miner: Miner = async (...args) => {
  const mined = await notemine(...args);
  mark('mined');
  return mined;
};

const client = createDeadhop({ ...(await config()), miner });
createRoot(document.getElementById('app')!).render(<PayButton client={client} amount={5000} memo="browser e2e (notemine)" pollMs={500} {...events} />);
