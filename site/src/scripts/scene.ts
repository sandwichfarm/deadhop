// The product's data flow as a small line-drawn scene:
//   customer ──✕── service          (no direct route: the service has no address)
//   customer → relays → service     (ciphertext only; the service dials out)
//   proof-of-work ring on the customer's request
//   xpub → fresh address per invoice (watch-only tree)
//   blocks stacking until the invoice is confirmed
// Every stage is a number from 0 to 1 that the scroll timeline drives.
import {
  AdditiveBlending,
  BufferAttribute,
  BufferGeometry,
  Color,
  EdgesGeometry,
  Group,
  IcosahedronGeometry,
  BoxGeometry,
  Line,
  LineBasicMaterial,
  LineSegments,
  PerspectiveCamera,
  Points,
  PointsMaterial,
  QuadraticBezierCurve3,
  Scene,
  Vector3,
  WebGLRenderer,
} from 'three';

export interface Stages {
  direct: number;
  relays: number;
  flow: number;
  work: number;
  tree: number;
  blocks: number;
}

export interface SceneHandle {
  stages: Stages;
  render(): void;
  start(): void;
  stop(): void;
  dispose(): void;
}

interface Palette {
  fg: Color;
  dim: Color;
  accent: Color;
}

function palette(): Palette {
  const css = getComputedStyle(document.documentElement);
  const read = (name: string, fallback: string) => new Color(css.getPropertyValue(name).trim() || fallback);
  return { fg: read('--scene-fg', '#e8e6e1'), dim: read('--scene-dim', '#5b5f66'), accent: read('--accent', '#f7931a') };
}

const CUSTOMER = new Vector3(-3.1, -0.6, 0);
const SERVICE = new Vector3(3.1, -0.6, 0);
const SHELL_RADIUS = 1.05;
const RELAYS = [
  new Vector3(-1.9, 1.7, -0.6),
  new Vector3(-0.8, 2.25, 0.4),
  new Vector3(0.4, 2.0, -0.8),
  new Vector3(1.5, 2.3, 0.3),
];
const PACKETS = 10;
const ADDRESSES = 7;
const BLOCKS = 6;
// Half extents of the diagram, with room for the pointer parallax.
const FIT_W = 4.7;
const FIT_H = 3.7;
const FIT_Y = -0.55;

function wire(geometry: BufferGeometry, color: Color, opacity = 1): LineSegments {
  const mat = new LineBasicMaterial({ color, transparent: true, opacity });
  return new LineSegments(new EdgesGeometry(geometry), mat);
}

function polyline(points: Vector3[], color: Color, opacity = 1): Line {
  const geo = new BufferGeometry().setFromPoints(points);
  return new Line(geo, new LineBasicMaterial({ color, transparent: true, opacity }));
}

function setOpacity(obj: Line | LineSegments | Points, value: number) {
  const m = obj.material as LineBasicMaterial | PointsMaterial;
  m.opacity = value;
  obj.visible = value > 0.002;
}

export function createScene(canvas: HTMLCanvasElement): SceneHandle {
  const colors = palette();
  const renderer = new WebGLRenderer({ canvas, antialias: true, alpha: true, powerPreference: 'low-power' });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));

  const scene = new Scene();
  const camera = new PerspectiveCamera(38, 1, 0.1, 100);

  const world = new Group();
  scene.add(world);

  // Customer: a small cube. Service: an icosahedron inside a closed shell.
  const customer = wire(new BoxGeometry(0.6, 0.6, 0.6), colors.fg);
  customer.position.copy(CUSTOMER);
  const service = wire(new IcosahedronGeometry(0.45, 0), colors.fg);
  service.position.copy(SERVICE);
  const shell = wire(new IcosahedronGeometry(SHELL_RADIUS, 1), colors.dim, 0.35);
  shell.position.copy(SERVICE);
  world.add(customer, service, shell);

  // Stage 1: the direct route, which stops dead at the shell.
  const stop = SERVICE.clone().add(CUSTOMER.clone().sub(SERVICE).setLength(SHELL_RADIUS));
  const directPoints = Array.from({ length: 64 }, (_, i) => CUSTOMER.clone().lerp(stop, i / 63));
  const direct = polyline(directPoints, colors.fg, 0.9);
  const cross = new LineSegments(
    new BufferGeometry().setFromPoints([
      stop.clone().add(new Vector3(-0.18, -0.18, 0)),
      stop.clone().add(new Vector3(0.18, 0.18, 0)),
      stop.clone().add(new Vector3(-0.18, 0.18, 0)),
      stop.clone().add(new Vector3(0.18, -0.18, 0)),
    ]),
    new LineBasicMaterial({ color: colors.accent, transparent: true, opacity: 0 }),
  );
  world.add(direct, cross);

  // Stage 2: relays, the links to them, and packets flowing through.
  const relayPoints = new Points(
    new BufferGeometry().setFromPoints(RELAYS),
    new PointsMaterial({ color: colors.fg, size: 0.2, transparent: true, opacity: 0 }),
  );
  const relayHalos = RELAYS.map((p) => {
    const h = wire(new IcosahedronGeometry(0.2, 0), colors.dim, 0);
    h.position.copy(p);
    return h;
  });
  const curves = RELAYS.flatMap((r) => [
    new QuadraticBezierCurve3(CUSTOMER, CUSTOMER.clone().lerp(r, 0.5).add(new Vector3(0, 0.6, 0)), r),
    new QuadraticBezierCurve3(r, r.clone().lerp(SERVICE, 0.5).add(new Vector3(0, 0.6, 0)), SERVICE),
  ]);
  const links = curves.map((c) => polyline(c.getPoints(40), colors.dim, 0));
  const packetGeo = new BufferGeometry();
  const packetPos = new Float32Array(PACKETS * 3);
  packetGeo.setAttribute('position', new BufferAttribute(packetPos, 3));
  const packets = new Points(
    packetGeo,
    new PointsMaterial({ color: colors.accent, size: 0.2, transparent: true, opacity: 0, blending: AdditiveBlending }),
  );
  world.add(relayPoints, packets, ...relayHalos, ...links);

  // Stage 3: proof of work, a ring of ticks spinning around the request.
  const ringPts: Vector3[] = [];
  for (let i = 0; i < 48; i++) {
    const a = (i / 48) * Math.PI * 2;
    const r = i % 4 === 0 ? 0.78 : 0.7;
    ringPts.push(new Vector3(Math.cos(a) * 0.62, Math.sin(a) * 0.62, 0), new Vector3(Math.cos(a) * r, Math.sin(a) * r, 0));
  }
  const ring = new LineSegments(
    new BufferGeometry().setFromPoints(ringPts),
    new LineBasicMaterial({ color: colors.accent, transparent: true, opacity: 0 }),
  );
  ring.position.copy(CUSTOMER);
  world.add(ring);

  // Stage 4: one account key, a fresh address per invoice.
  const xpub = SERVICE.clone().add(new Vector3(0, -1.5, 0));
  const trunk = polyline([SERVICE.clone().add(new Vector3(0, -0.45, 0)), xpub], colors.dim, 0);
  const branches = Array.from({ length: ADDRESSES }, (_, i) => {
    const x = xpub.x - 1.8 + (i * 3.6) / (ADDRESSES - 1);
    const leaf = new Vector3(x, xpub.y - 1.0, 0.2 * Math.sin(i));
    return { line: polyline([xpub, new Vector3(x, xpub.y - 0.5, 0), leaf], colors.dim, 0), leaf };
  });
  const leaves = new Points(
    new BufferGeometry().setFromPoints(branches.map((b) => b.leaf)),
    new PointsMaterial({ color: colors.accent, size: 0.2, transparent: true, opacity: 0 }),
  );
  world.add(trunk, leaves, ...branches.map((b) => b.line));

  // Stage 5: blocks stacking on the chain until the payment is confirmed.
  const blocks = Array.from({ length: BLOCKS }, (_, i) => {
    const b = wire(new BoxGeometry(0.42, 0.42, 0.42), i === BLOCKS - 1 ? colors.accent : colors.fg, 0);
    b.position.set(-2.6 + i * 0.62, -3.05, 0);
    return b;
  });
  const chain = polyline([new Vector3(-2.6, -3.05, 0), new Vector3(-2.6 + (BLOCKS - 1) * 0.62, -3.05, 0)], colors.dim, 0);
  world.add(chain, ...blocks);

  const stages: Stages = { direct: 0, relays: 0, flow: 0, work: 0, tree: 0, blocks: 0 };
  const pointer = { x: 0, y: 0 };
  const onPointer = (e: PointerEvent) => {
    pointer.x = e.clientX / window.innerWidth - 0.5;
    pointer.y = e.clientY / window.innerHeight - 0.5;
  };
  window.addEventListener('pointermove', onPointer, { passive: true });

  function resize() {
    const { clientWidth: w, clientHeight: h } = canvas;
    if (!w || !h) return;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    // Fit the whole diagram (half extents FIT_W x FIT_H around FIT_Y) in any aspect ratio.
    const t = Math.tan((camera.fov * Math.PI) / 360);
    camera.position.set(0, FIT_Y, Math.max(FIT_W / (t * camera.aspect), FIT_H / t));
    camera.lookAt(0, FIT_Y, 0);
    camera.updateProjectionMatrix();
    // setSize clears the canvas; redraw so a static (reduced-motion) scene survives resizes.
    if (ready) render(performance.now());
  }
  let ready = false;
  const ro = new ResizeObserver(resize);
  ro.observe(canvas);
  resize();

  const clock = { t: 0, last: 0 };
  ready = true;

  function render(now = 0) {
    const dt = clock.last ? Math.min((now - clock.last) / 1000, 0.05) : 0;
    clock.last = now;
    clock.t += dt;
    const t = clock.t;
    const s = stages;

    world.rotation.y += (pointer.x * 0.18 - world.rotation.y) * 0.05;
    world.rotation.x += (pointer.y * 0.08 - world.rotation.x) * 0.05;
    service.rotation.y = t * 0.35;
    service.rotation.x = t * 0.2;
    customer.rotation.y = -t * 0.3;

    // Stage 1: draw the line up to the shell, then flash the refusal.
    const drawn = Math.max(0, Math.min(1, s.direct * 1.25));
    direct.geometry.setDrawRange(0, Math.floor(drawn * 64));
    setOpacity(direct, s.direct > 0 ? 0.9 * Math.min(1, s.direct * 3) : 0);
    setOpacity(cross, Math.max(0, (s.direct - 0.75) * 4));
    setOpacity(shell, 0.25 + 0.5 * Math.max(0, (s.direct - 0.75) * 4));

    // Stage 2.
    setOpacity(relayPoints, s.relays);
    relayHalos.forEach((h, i) => {
      setOpacity(h, s.relays * 0.6);
      h.rotation.y = t * 0.4 + i;
    });
    links.forEach((l) => setOpacity(l, s.relays * 0.45));
    for (let i = 0; i < PACKETS; i++) {
      const relay = i % RELAYS.length;
      const phase = (t * 0.22 + i / PACKETS) % 1;
      const leg = phase < 0.5 ? curves[relay * 2] : curves[relay * 2 + 1];
      const p = leg.getPoint((phase % 0.5) * 2);
      packetPos.set([p.x, p.y, p.z], i * 3);
    }
    packetGeo.attributes.position.needsUpdate = true;
    setOpacity(packets, s.flow);

    // Stage 3.
    setOpacity(ring, s.work);
    ring.rotation.z = -t * (1.5 + 4 * s.work);
    ring.scale.setScalar(0.9 + 0.1 * Math.sin(t * 6) * s.work);

    // Stage 4: grow the branches one by one.
    setOpacity(trunk, Math.min(1, s.tree * 3) * 0.8);
    branches.forEach((b, i) => setOpacity(b.line, Math.max(0, Math.min(1, s.tree * ADDRESSES - i)) * 0.7));
    (leaves.geometry as BufferGeometry).setDrawRange(0, Math.floor(s.tree * ADDRESSES + 0.001));
    setOpacity(leaves, s.tree > 0 ? 1 : 0);

    // Stage 5.
    setOpacity(chain, s.blocks * 0.6);
    blocks.forEach((b, i) => {
      const v = Math.max(0, Math.min(1, s.blocks * BLOCKS - i));
      setOpacity(b, v);
      b.position.y = -3.05 + (1 - v) * 0.6;
      b.rotation.y = (1 - v) * 1.2;
    });

    renderer.render(scene, camera);
  }

  let raf = 0;
  const loop = (now: number) => {
    render(now);
    raf = requestAnimationFrame(loop);
  };

  return {
    stages,
    render: () => render(performance.now()),
    start() {
      if (!raf) raf = requestAnimationFrame(loop);
    },
    stop() {
      cancelAnimationFrame(raf);
      raf = 0;
      clock.last = 0;
    },
    dispose() {
      cancelAnimationFrame(raf);
      ro.disconnect();
      window.removeEventListener('pointermove', onPointer);
      renderer.dispose();
    },
  };
}
