# Website

The public landing page is a static site in `site/`, served by Vercel at
`https://gezi.arieszhou.com/`. It requires no frontend dependencies or build.
The root `vercel.json` selects `site/` and configures security headers.
`.vercelignore` excludes the application and development files from CLI uploads.

The domain uses externally managed Cloudflare DNS with a CNAME to Vercel.
Retain the existing nameservers and unrelated DNS records. Check the project's
current domain settings before changing the CNAME target.

## Local checks

Run `python3 -B scripts/test_site.py`, then serve `site/` with a local HTTP server.
Check desktop and mobile layouts, keyboard navigation, the install anchor and
both clipboard success and failure. Clipboard access requires localhost or HTTPS.

The terminal specimen is actual Gezi 0.1.9 ANSI output generated from
`site/reading.md` with `--ansi --theme default-dark --width 64` and width 34.
The page translates ANSI colors to HTML. It is not an interactive browser reader.
Keep the version label and rendered specimen consistent when updating it.

## Deployment

Use the same Vercel account and team as the other product sites. Link this
repository to the `gezi` project, then deploy a preview:

```sh
vercel link --project gezi
vercel deploy --target preview
```

Check the returned deployment URL before publishing. Production promotion and
binding the public domain require maintainer confirmation immediately before
publication. Do not change the Rust package version for website-only work.

The production page includes canonical, Open Graph and Twitter metadata for
`gezi.arieszhou.com`. The sharing image is illustrative. Preserve the official
logo and approved public maker attribution. Do not publish exploration routes,
local review reports, credentials or machine paths.
