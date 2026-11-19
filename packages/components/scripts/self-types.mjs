// JSR reads a module's types from a `@ts-self-types` comment. Point every
// built React and Solid module at the declaration file emitted next to it.
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

function mark(file, types) {
  const code = readFileSync(file, 'utf8');
  if (code.includes('@ts-self-types')) return;
  writeFileSync(file, `// @ts-self-types="${types}"\n${code}`);
}

for (const dir of ['dist/react', 'dist/solid/source']) {
  for (const name of readdirSync(dir)) {
    if (!/\.jsx?$/.test(name)) continue;
    const dts = name.replace(/\.jsx?$/, '.d.ts');
    if (existsSync(join(dir, dts))) mark(join(dir, name), `./${dts}`);
  }
}
// The compiled Solid bundle has the same exports as the source entry.
mark('dist/solid/index.js', './source/index.d.ts');
