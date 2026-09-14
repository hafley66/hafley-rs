# Building these docs

The published site has two parts, built into one artifact:

- an mdBook at the site root, with source in `docs/book/`.
- rustdoc for every workspace crate under `api/`, generated with
  `cargo doc --workspace --no-deps --locked` using each crate's default
  features.

## Local build

```bash
just docs
```

writes the whole site to `target/docs-site`. The recipe calls
`scripts/docs/0_build_site.sh`, which:

1. validates the destination with `scripts/docs/site_paths.py`, which refuses
   to wipe anything but a subdirectory of a Cargo target directory,
2. builds the book with `mdbook build docs/book` into the site root,
3. runs `cargo doc --workspace --no-deps --locked`,
4. copies `target/doc` to `site/api`, preserving rustdoc's assets and search
   index,
5. generates `api/index.html` from `cargo metadata` and the emitted targets,
6. runs the coverage and link checker.

Destination guard tests are deterministic and need no build:

```bash
python3 scripts/docs/3_test_guards.py   # or: just docs-guard
```

The scripts can be run directly:

```bash
bash scripts/docs/0_build_site.sh target/docs-site
python3 scripts/docs/3_test_guards.py
python3 scripts/docs/1_index_api.py --repo-root . --doc-dir target/doc \
  --out target/docs-site/api/index.html
python3 scripts/docs/2_check_site.py --repo-root . --site-dir target/docs-site \
  --doc-dir target/doc
```

## Publishing

`.github/workflows/docs.yml` builds the site on every pull request and publishes
it on pushes to `main` and on manual runs. It uses the official GitHub Pages
actions:

| action | job |
| --- | --- |
| `actions/configure-pages` | page metadata for the deploy |
| `actions/upload-pages-artifact` | package `target/docs-site` as the Pages artifact |
| `actions/deploy-pages` | publish the artifact |

Pull requests build and check the site but do not upload or deploy. The publish
job needs `pages: write` and `id-token: write`; every other job reads contents
only. mdBook is pinned to 0.5.4 through `taiki-e/install-action`. No generated
HTML is committed to a source branch.

The site is served from the project Pages base path `/hafley-rs/`. Every link
the book and the landing page emit is document-relative, so the artifact works
under that base path without a rewrite step.

## Commit conventions

`CONTRIBUTING.md` defines the commit subjects release automation reads. `docs`,
`test`, `build`, `ci`, `chore`, and `style` do not request a release; `feat`,
`fix`, `perf`, and `refactor` do.
