# Rebased icon sources

The icon is assembled locally from separate layers, not generated as a single image.

1. `background-source.webp`: unchanged copy of the supplied local artwork (2880x1664).
2. `background-40.png`: the head-to-waist square (`left 920, top 0, 1280x1280`) of the
   source, rounded corners and alpha scaled by **0.4** (maximum 102/255). No opaque
   background is baked into this layer.
3. `frame.svg`: orange rounded-square border, independent of the photo and mark.
4. `rebased-mark.svg`: the node graph from Rebased's
   `community-resources/resources/rebased.svg`, recoloured with the Rebased orange
   gradient plus a light rim and dark halo so it stays legible over the photo.

The final 1024px RGBA master is `../icon.png`. `../../src-tauri/icons/icon.png` (256px)
and the multi-resolution `icon.ico` are exported from it.

## Rebuild

From this directory, using Node.js 20.9+:

```powershell
npm ci
npm run prepare-background
npm run compose
npm run verify
```

To also refresh the sibling host's bundled icon:

```powershell
npm run compose -- ../../../background-studio
npm run verify -- ../../../background-studio
```

To replace the source photo, pass its path to `npm run prepare-background --` and
adjust the crop box in `prepare-background.mjs`.

`preview-dark.png` and `preview-light.png` are ignored opaque previews only;
the final exported icons retain their alpha channels.
