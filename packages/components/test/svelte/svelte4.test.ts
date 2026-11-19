// The Svelte 4 components, here compiled by Svelte 5 in legacy mode. The real
// Svelte 4 runtime runs the same scenarios in test-svelte4/.
import * as kit from '../../src/svelte4/index.js';
import { scenarios } from './scenarios.ts';

scenarios('Svelte 4 syntax', kit);
