# Publishing

GitHub repository settings must configure Pages to deploy from GitHub Actions (Settings > Pages > Build and deployment > Source: "GitHub Actions"). This is a one-time, admin-only repository setting this change cannot make itself — until it's set, `docs-pages.yml` will build successfully but the deploy job will fail.

Pushes to `develop` that change `docs/site` or `.github/workflows/docs-pages.yml` build and deploy the site.

Before merging, run `bundle install` and `bundle exec jekyll build` from `docs/site`. Confirm the generated root redirects to `latest`, and open each path listed in `_data/versions.yml`.

After a successful deployment, verify the published root URL, `/latest/`, and the version selector (even with one entry) resolve correctly.
