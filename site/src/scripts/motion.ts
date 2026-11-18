// Scroll timeline and reveals. Everything here is progressive enhancement: the
// HTML is complete and visible without it, and nothing runs when the reader
// asks for reduced motion (the scene then shows its final state once).
import { gsap } from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import type { SceneHandle, Stages } from './scene';

const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

function webgl(): boolean {
  try {
    const c = document.createElement('canvas');
    return !!(c.getContext('webgl2') || c.getContext('webgl'));
  } catch {
    return false;
  }
}

async function loadScene(): Promise<SceneHandle | null> {
  const canvas = document.querySelector<HTMLCanvasElement>('#scene');
  if (!canvas || !webgl()) return null;
  const { createScene } = await import('./scene');
  const handle = createScene(canvas);
  canvas.classList.add('is-ready');
  return handle;
}

// One master timeline over the story; each step owns one segment.
function storyTimeline(stages: Stages) {
  const tl = gsap.timeline({
    defaults: { ease: 'none', duration: 1 },
    scrollTrigger: { trigger: '#story', start: 'top 60%', end: 'bottom 70%', scrub: 0.8 },
  });
  tl.to(stages, { direct: 1 }, 0)
    .to(stages, { direct: 0, relays: 1 }, 1)
    .to(stages, { flow: 1 }, 1.3)
    .to(stages, { work: 1 }, 2)
    .to(stages, { work: 0, tree: 1 }, 3)
    .to(stages, { blocks: 1 }, 4);
  return tl;
}

function reveals() {
  // Starting states are applied here, never in CSS, so content without JS is visible.
  gsap.utils.toArray<HTMLElement>('[data-reveal]').forEach((el) => {
    gsap.from(el, {
      y: 28,
      autoAlpha: 0,
      duration: 0.9,
      ease: 'power3.out',
      scrollTrigger: { trigger: el, start: 'top 85%', once: true },
    });
  });
  gsap.utils.toArray<HTMLElement>('[data-stagger]').forEach((list) => {
    gsap.from(list.children, {
      y: 18,
      autoAlpha: 0,
      duration: 0.7,
      ease: 'power2.out',
      stagger: 0.06,
      scrollTrigger: { trigger: list, start: 'top 80%', once: true },
    });
  });
  // Mark the step being read so the copy and the scene stay in step.
  gsap.utils.toArray<HTMLElement>('.step').forEach((step) => {
    ScrollTrigger.create({
      trigger: step,
      start: 'top 55%',
      end: 'bottom 45%',
      toggleClass: { targets: step, className: 'is-active' },
    });
  });
}

async function main() {
  const scene = await loadScene();
  if (reduced) {
    if (scene) {
      Object.assign(scene.stages, { direct: 0, relays: 1, flow: 1, work: 0, tree: 1, blocks: 1 });
      scene.render();
    }
    return;
  }
  gsap.registerPlugin(ScrollTrigger);
  reveals();
  if (scene) {
    storyTimeline(scene.stages);
    ScrollTrigger.create({
      trigger: '#story',
      start: 'top 75%',
      end: 'bottom top',
      toggleClass: { targets: '#scene', className: 'in-story' },
    });
    // Only animate while the canvas is on screen and the tab is visible.
    const canvas = document.querySelector('#scene')!;
    new IntersectionObserver(([e]) => (e.isIntersecting ? scene.start() : scene.stop())).observe(canvas);
    document.addEventListener('visibilitychange', () => (document.hidden ? scene.stop() : scene.start()));
  }
}

void main();
