import { copyFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';

const assetDirectory = path.dirname(fileURLToPath(import.meta.url));
const pluginDirectory = path.resolve(assetDirectory, '../..');
const iconDirectory = path.join(pluginDirectory, 'src-tauri/icons');
const hostDirectory = process.argv[2] ? path.resolve(process.argv[2]) : null;

// Geometry copied from Rebased's community-resources/resources/rebased.svg (16x16 grid).
const markScale = 48;
const markTranslateX = 512 - 8.15 * markScale;
const markTranslateY = 512 - 8 * markScale;
const nodes = [[6, 3], [6, 8], [6, 13], [10.3, 5.5], [10.3, 10.5]];
const edges = 'M10.5 10.5L6 8L10.5 5.5L6 3V13';
const nodeRadius = 1.7;
const edgeWidth = 1;

function markLayer(grow, paint) {
  const circles = nodes
    .map(([cx, cy]) => `<circle cx="${cx}" cy="${cy}" r="${nodeRadius + grow}" fill="${paint}"/>`)
    .join('');
  return `<path d="${edges}" fill="none" stroke="${paint}" stroke-width="${edgeWidth + grow * 2}" stroke-linejoin="round" stroke-linecap="round"/>${circles}`;
}

const markSource = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="orange" gradientUnits="userSpaceOnUse" x1="4.3" y1="14.7" x2="12" y2="1.3">
      <stop offset="0" stop-color="#FF7A1A"/>
      <stop offset="0.55" stop-color="#FF5A00"/>
      <stop offset="1" stop-color="#F23A00"/>
    </linearGradient>
  </defs>
  <g transform="translate(${markTranslateX} ${markTranslateY}) scale(${markScale})">
    <g opacity="0.55">${markLayer(0.34, '#1B0C04')}</g>
    ${markLayer(0.14, '#FFE3CF')}
    ${markLayer(0, 'url(#orange)')}
  </g>
</svg>`;
await writeFile(path.join(assetDirectory, 'rebased-mark.svg'), `${markSource}\n`);

const frameSource = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <rect x="74" y="74" width="876" height="876" rx="208" fill="none" stroke="#3D1A06" stroke-width="22" stroke-opacity="0.35"/>
  <rect x="74" y="74" width="876" height="876" rx="208" fill="none" stroke="#FF6A1A" stroke-width="10" stroke-opacity="0.90"/>
  <rect x="80" y="80" width="864" height="864" rx="202" fill="none" stroke="#FFD9BF" stroke-width="1.5" stroke-opacity="0.40"/>
</svg>`;
await writeFile(path.join(assetDirectory, 'frame.svg'), `${frameSource}\n`);

const composedIcon = await sharp(path.join(assetDirectory, 'background-40.png'))
  .composite([
    { input: Buffer.from(frameSource) },
    { input: Buffer.from(markSource) },
  ])
  .png()
  .toBuffer();
const masterPath = path.join(pluginDirectory, 'assets/icon.png');
await writeFile(masterPath, composedIcon);

for (const [theme, background] of [['dark', '#161D27'], ['light', '#F2F5F9']]) {
  await sharp(composedIcon).flatten({ background }).resize(512, 512)
    .png().toFile(path.join(assetDirectory, `preview-${theme}.png`));
}

for (const filename of await readdir(iconDirectory)) {
  if (!filename.endsWith('.png')) continue;
  const outputPath = path.join(iconDirectory, filename);
  const metadata = await sharp(outputPath).metadata();
  await sharp(composedIcon).resize(metadata.width, metadata.height)
    .png().toFile(outputPath);
}

async function createWindowsIcon() {
  const dimensions = [16, 24, 32, 48, 64, 128, 256];
  const images = await Promise.all(dimensions.map((dimension) =>
    sharp(composedIcon).resize(dimension, dimension).png().toBuffer(),
  ));
  // ICO directory entries reference lossless PNG payloads supported by Windows.
  const directory = Buffer.alloc(6 + dimensions.length * 16);
  directory.writeUInt16LE(1, 2);
  directory.writeUInt16LE(dimensions.length, 4);
  let imageOffset = directory.length;
  for (const [imageIndex, dimension] of dimensions.entries()) {
    const entryOffset = 6 + imageIndex * 16;
    directory[entryOffset] = dimension === 256 ? 0 : dimension;
    directory[entryOffset + 1] = dimension === 256 ? 0 : dimension;
    directory.writeUInt16LE(1, entryOffset + 4);
    directory.writeUInt16LE(32, entryOffset + 6);
    directory.writeUInt32LE(images[imageIndex].length, entryOffset + 8);
    directory.writeUInt32LE(imageOffset, entryOffset + 12);
    imageOffset += images[imageIndex].length;
  }
  return Buffer.concat([directory, ...images]);
}

await writeFile(path.join(iconDirectory, 'icon.ico'), await createWindowsIcon());

if (hostDirectory) {
  const hostIcon = await sharp(composedIcon).resize(512, 512).png().toBuffer();
  await writeFile(path.join(hostDirectory, 'src-tauri/resources/rebased.png'), hostIcon);
  await copyFile(
    path.join(hostDirectory, 'src-tauri/resources/rebased.png'),
    path.join(hostDirectory, 'public/plugins/rebased.png'),
  );
}

console.log(`Composed ${masterPath}: separate 40% photo, orange frame and Rebased mark.`);
console.log('Exported Windows PNGs and ICO sizes: 16, 24, 32, 48, 64, 128, 256.');
