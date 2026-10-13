// Downloads Intel PresentMon 2.6.0 (MIT licence) for bundling. Build time only: Kestrel never downloads anything
// at runtime. A file whose SHA-256 differs from the pinned one is refused. Run from app/: npm run fetch:presentmon
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const out = join(dirname(fileURLToPath(import.meta.url)), '..', 'src-tauri', 'binaries');
const files = [
  {
    name: 'PresentMon-2.6.0-x64.exe',
    url: 'https://github.com/GameTechDev/PresentMon/releases/download/v2.6.0/PresentMon-2.6.0-x64.exe',
    sha256: 'b2a706bc6ad475749e3b7e3409263aa1e6906d45bdcf993f6dbc0f660188f1af',
  },
  {
    name: 'PresentMon-LICENSE.txt',
    url: 'https://raw.githubusercontent.com/GameTechDev/PresentMon/v2.6.0/LICENSE.txt',
    sha256: '4c949341b1893c8c6ad82f7fb4eedf622cd1fd9c22a9af8f19b2dac19d1947b6',
  },
];

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

await mkdir(out, { recursive: true });
for (const file of files) {
  const path = join(out, file.name);
  const existing = await readFile(path).catch(() => null);
  if (existing && sha256(existing) === file.sha256) {
    console.log(`${file.name}: already here`);
    continue;
  }
  const response = await fetch(file.url);
  if (!response.ok) throw new Error(`${file.name}: download failed (${response.status})`);
  const bytes = Buffer.from(await response.arrayBuffer());
  const actual = sha256(bytes);
  if (actual !== file.sha256) throw new Error(`${file.name}: SHA-256 ${actual} doesn't match the pinned ${file.sha256}`);
  await writeFile(path, bytes);
  console.log(`${file.name}: downloaded and verified (${bytes.length} bytes)`);
}
