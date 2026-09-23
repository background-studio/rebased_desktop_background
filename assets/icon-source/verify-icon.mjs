import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';

const assetDirectory = path.dirname(fileURLToPath(import.meta.url));
const pluginDirectory = path.resolve(assetDirectory, '../..');

async function readPixels(relativePath) {
  return sharp(path.resolve(assetDirectory, relativePath))
    .ensureAlpha().raw().toBuffer({ resolveWithObject: true });
}

function readAlpha(image, column, row) {
  return image.data[(row * image.info.width + column) * 4 + 3];
}

const background = await readPixels('background-40.png');
const composite = await readPixels('../icon.png');
for (const image of [background, composite]) {
  assert.equal(image.info.width, 1024);
  assert.equal(image.info.height, 1024);
  assert.equal(readAlpha(image, 0, 0), 0, 'Outside corners must be transparent.');
}
let maximumBackgroundAlpha = 0;
for (let alphaOffset = 3; alphaOffset < background.data.length; alphaOffset += 4) {
  maximumBackgroundAlpha = Math.max(maximumBackgroundAlpha, background.data[alphaOffset]);
}
assert.equal(maximumBackgroundAlpha, 102, 'Photo opacity must be exactly 40%.');
// (10.3, 8) on the 16px grid sits between the two right-hand nodes.
assert.equal(readAlpha(composite, 615, 512), 102, 'The gap inside the mark must reveal the 40% photo.');
// (6, 8) on the 16px grid is the centre of the middle node.
assert.equal(readAlpha(composite, 409, 512), 255, 'The middle node must be opaque.');

const iconPng = await sharp(path.join(pluginDirectory, 'src-tauri/icons/icon.png')).metadata();
assert.equal(iconPng.width, 256);
assert.equal(iconPng.hasAlpha, true);

const windowsIcon = await readFile(path.join(pluginDirectory, 'src-tauri/icons/icon.ico'));
const expectedDimensions = [16, 24, 32, 48, 64, 128, 256];
assert.equal(windowsIcon.readUInt16LE(0), 0);
assert.equal(windowsIcon.readUInt16LE(2), 1);
assert.equal(windowsIcon.readUInt16LE(4), expectedDimensions.length);
for (const [imageIndex, dimension] of expectedDimensions.entries()) {
  const entryOffset = 6 + imageIndex * 16;
  const imageLength = windowsIcon.readUInt32LE(entryOffset + 8);
  const imageOffset = windowsIcon.readUInt32LE(entryOffset + 12);
  assert.ok(imageOffset + imageLength <= windowsIcon.length);
  const metadata = await sharp(windowsIcon.subarray(imageOffset, imageOffset + imageLength))
    .metadata();
  assert.equal(metadata.width, dimension);
  assert.equal(metadata.height, dimension);
  assert.equal(metadata.hasAlpha, true);
}

if (process.argv[2]) {
  const hostDirectory = path.resolve(process.argv[2]);
  const bundledIcon = await readFile(path.join(hostDirectory, 'src-tauri/resources/rebased.png'));
  const publicIcon = await readFile(path.join(hostDirectory, 'public/plugins/rebased.png'));
  assert.deepEqual(bundledIcon, publicIcon);
  const catalog = JSON.parse(await readFile(
    path.join(hostDirectory, 'src-tauri/resources/catalog.json'), 'utf8',
  ));
  assert.equal(catalog.plugins.find((plugin) => plugin.id === 'rebased').icon, 'rebased.png');
}

console.log('PASS: 40% photo, transparent corners/gap, opaque mark, seven ICO sizes and host assets.');
