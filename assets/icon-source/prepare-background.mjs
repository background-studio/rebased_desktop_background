import { copyFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';

const assetDirectory = path.dirname(fileURLToPath(import.meta.url));
const sourcePath = path.join(assetDirectory, 'background-source.webp');
const suppliedSourcePath = process.argv[2];

if (suppliedSourcePath && path.resolve(suppliedSourcePath) !== sourcePath) {
  await copyFile(suppliedSourcePath, sourcePath);
}

const canvasSize = 1024;
const tileInset = 64;
const tileSize = canvasSize - tileInset * 2;
const cornerRadius = 220;
const backgroundOpacity = 0.4;
// Head-to-waist square picked from the 2880x1664 source.
const crop = { left: 920, top: 0, width: 1280, height: 1280 };

const metadata = await sharp(sourcePath).metadata();
if (metadata.width !== 2880 || metadata.height !== 1664) {
  throw new Error(
    `The crop box is tuned for a 2880x1664 source, got ${metadata.width}x${metadata.height}.`,
  );
}

// Keep the photo's alpha separate from the foreground mark and orange frame.
const roundedMask = Buffer.from(
  `<svg width="${tileSize}" height="${tileSize}">
    <rect width="${tileSize}" height="${tileSize}" rx="${cornerRadius}" fill="white"/>
  </svg>`,
);

const croppedTile = await sharp(sourcePath)
  .extract(crop)
  .resize(tileSize, tileSize)
  .ensureAlpha()
  .png()
  .toBuffer();

const { data: backgroundPixels, info: backgroundInfo } = await sharp(croppedTile)
  .composite([{ input: roundedMask, blend: 'dest-in' }])
  .raw()
  .toBuffer({ resolveWithObject: true });

for (let alphaOffset = 3; alphaOffset < backgroundPixels.length; alphaOffset += 4) {
  backgroundPixels[alphaOffset] = Math.round(
    backgroundPixels[alphaOffset] * backgroundOpacity,
  );
}

await sharp(backgroundPixels, { raw: backgroundInfo })
  .extend({
    top: tileInset,
    bottom: tileInset,
    left: tileInset,
    right: tileInset,
    background: { r: 0, g: 0, b: 0, alpha: 0 },
  })
  .png()
  .toFile(path.join(assetDirectory, 'background-40.png'));

console.log('Prepared background-40.png: 1024x1024, maximum alpha 102/255 (40%).');
