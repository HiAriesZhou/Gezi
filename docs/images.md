# Images in the inline reader

On a direct Kaku terminal, `tmd article.md` displays static PNG, JPEG and SVG
images inside the document. Both Markdown `![alt](path)` and HTML `<img src>`
are supported. A `<picture>` uses its `<img>` fallback; media-specific
`<source>` selection and browser CSS are not implemented.

```sh
tmd article.md
tmd --remote-images off article.md
tmd --images off article.md
```

Remote HTTP(S) images load by default. This contacts the image hosts named in
the document. `--remote-images off` keeps local images and does not fetch
remote images. `--images off` disables all image loading and graphics output.

Local paths resolve relative to the Markdown file, including percent-encoded
spaces and local paths with query/fragment suffixes. File extensions are not
required: content determines the format. URL credentials, `file:` and `data:`
URLs are not supported.

Images keep their proportions, respect positive integer pixel width/height
hints and fit within the document pane. Adjacent images such as badges share
rows when they fit. Images are laid out in rows rather than with arbitrary
browser-style text wrapping. Table cells retain textual alt labels.

Loading and decoding run on at most two background workers. Visible images
and one adjacent viewport are eligible for loading. Loading new dimensions
preserves the current text/source anchor; when viewing an image, resizing
preserves the relative position within it. Scrolling, opening the outline,
help, search and resizing clip or hide placements at the document boundaries.
Only this reader's image IDs are cleaned up on exit.

The session has an accounted 64 MiB raster cache, at most 1,024 distinct
resources, an 8 MiB input limit per image, a 16 megapixel raster decoding limit,
and an eight-second HTTP timeout with at most three redirects. Raster output
is downsampled to at most 2,048 pixels per side. At most 256 prepared size
variants are cached; cache accounting also includes their decoded terminal
footprint. SVG has a separate 2 MiB / 20,000-tag input limit. Compressed SVGZ
and DTD declarations are rejected. SVG scripts, external images and embedded images are
not evaluated or loaded. A small macOS system font set supplies SVG text.
Cached images are kept in memory for this process only.

Missing, invalid, oversized and failed images keep their alt text with a short
failure label. Remote images disabled by the option keep ordinary alt labels.
Animated formats, image enlargement, streaming graphics and non-Kaku image
backends are not included. Redirected output, `--ansi`, the native reader and
unsupported/multiplexed terminals retain textual placeholders.

Kaku's Kitty graphics support must be enabled. Detection uses the direct Kaku
environment, rather than injecting a capability query into keyboard input.
If graphics are disabled in Kaku configuration, use `--images off`.

Automated protocol tests complement visual testing; they cannot establish
whether a particular Kaku build displays images correctly.
