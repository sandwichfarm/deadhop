// Tiny entry point: the page is complete without JavaScript. Motion (GSAP and
// three.js) is fetched only after the page has loaded and the browser is idle,
// so it never competes with the first paint.
const idle = (cb: () => void) =>
  'requestIdleCallback' in window ? window.requestIdleCallback(cb, { timeout: 2000 }) : setTimeout(cb, 300);

const start = () => idle(() => void import('./motion'));

if (document.readyState === 'complete') start();
else window.addEventListener('load', start, { once: true });
