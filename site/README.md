# Marketing site

Static landing page for **Token-Efficient Agent Harness Lab**.

## Local preview

```bash
# any static server
python3 -m http.server 4173 --directory site
# open http://127.0.0.1:4173
```

## Vercel

Root `vercel.json` sets `outputDirectory` to `site`. Deploy from repo root:

```bash
vercel --prod --yes
```

Or connect the GitHub repo in the Vercel dashboard with **Root Directory** empty (repo root) or `site`.
