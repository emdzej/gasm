# "Built for gasm" badge

If your game ships as a gasm guest, you can put this badge on its store page,
README or website. It tells players that the `.wasm` runs on any gasm runner:
native, browser or headless.

<div class="badge-preview">
  <div class="light"><img src="/badge/built-for-gasm-light.svg" alt="Built for gasm"><img src="/badge/built-for-gasm-flat.svg" alt="built for: gasm"></div>
  <div class="dark"><img src="/badge/built-for-gasm-dark.svg" alt="Built for gasm"><img src="/badge/built-for-gasm-flat.svg" alt="built for: gasm"></div>
</div>

| Variant | Size | Use on | URL |
|---|---|---|---|
| Light | 120×44 | light pages | `https://gasm.emdzej.pl/badge/built-for-gasm-light.svg` |
| Dark | 120×44 | dark pages | `https://gasm.emdzej.pl/badge/built-for-gasm-dark.svg` |
| Flat | 118×20 | READMEs, next to other shields | `https://gasm.emdzej.pl/badge/built-for-gasm-flat.svg` |

All three are plain SVGs with no external fonts or scripts, so they scale
cleanly and work in an `<img>`. Link the badge to `https://gasm.emdzej.pl`.

## Markdown

```md
[![Built for gasm](https://gasm.emdzej.pl/badge/built-for-gasm-flat.svg)](https://gasm.emdzej.pl)
```

## HTML

```html
<a href="https://gasm.emdzej.pl">
  <img src="https://gasm.emdzej.pl/badge/built-for-gasm-light.svg" alt="Built for gasm" width="120" height="44">
</a>
```

This version switches to the dark badge when the viewer's system is in dark mode:

```html
<a href="https://gasm.emdzej.pl">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://gasm.emdzej.pl/badge/built-for-gasm-dark.svg">
    <img src="https://gasm.emdzej.pl/badge/built-for-gasm-light.svg" alt="Built for gasm" width="120" height="44">
  </picture>
</a>
```

## When to use it

Use the badge when the product is a gasm guest: a single `.wasm` module that
imports the `gasm` ABI (plus any of `gasm:gfx`, `gasm:net`, `gasm:storage`)
and runs in the reference runners. The language and SDK don't matter.

Please don't recolor, stretch or crop the badge, or use it for runners and
tools (it describes games). You can host a copy yourself; the files are MIT
licensed like the rest of the repository.
