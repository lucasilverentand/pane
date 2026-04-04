# pane docs site

This directory contains the Astro and Starlight documentation site for `pane`.

## Commands

Run commands from `site/`:

| Command | Action |
| :--- | :--- |
| `bun install` | Install dependencies |
| `bun run dev` | Start the local dev server |
| `bun run build` | Build the static site into `dist/` |
| `bun run preview` | Preview the production build locally |
| `bun run astro -- --help` | Show Astro CLI help |

## Structure

```text
site/
├── public/
├── src/
│   ├── assets/
│   ├── content/
│   │   └── docs/
│   └── styles/
├── astro.config.mjs
├── package.json
└── tsconfig.json
```

Docs pages live in `src/content/docs/`. Starlight generates routes from the
directory structure.
