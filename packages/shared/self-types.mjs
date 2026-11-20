// JSR reads a module's types from a `@ts-self-types` comment.
//   node self-types.mjs DIR...            mark every .js/.jsx in DIR with the .d.ts beside it,
//                                         and point the .d.ts files' relative imports at .js
//                                         (tsc keeps the sources' .ts/.tsx, which Deno can't resolve)
//   node self-types.mjs --entry FILE TYPES  mark one file with the given declaration
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

function mark(file, types) {
  const code = readFileSync(file, 'utf8');
  if (code.includes('@ts-self-types')) return;
  writeFileSync(file, `// @ts-self-types="${types}"\n${code}`);
}

const args = process.argv.slice(2);
if (args[0] === '--entry') {
  mark(args[1], args[2]);
} else {
  for (const dir of args) {
    for (const name of readdirSync(dir)) {
      if (name.endsWith('.d.ts')) {
        const file = join(dir, name);
        const code = readFileSync(file, 'utf8');
        const emitted = (base) => (existsSync(join(dir, `${base}.jsx`)) ? '.jsx' : '.js');
        writeFileSync(file, code.replace(/(from\s+['"])(\.{1,2}\/[^'"]+)\.tsx?(['"])/g, (_, a, base, b) => a + base + emitted(base) + b));
        continue;
      }
      if (!/\.jsx?$/.test(name)) continue;
      const dts = name.replace(/\.jsx?$/, '.d.ts');
      if (existsSync(join(dir, dts))) mark(join(dir, name), `./${dts}`);
    }
  }
}
