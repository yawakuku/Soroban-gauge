# CI

## Workflow

```yaml
name: gauge
on: pull_request
permissions:
  contents: read
  pull-requests: write   # lets the action post or update its comment
jobs:
  gauge:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@<full commit SHA>
      # Build your contract to Wasm first, with whatever you normally use, e.g.:
      - run: stellar contract build
      - uses: OWNER/soroban-gauge@v0.1.0
        with:
          config: soroban-gauge.toml
          fail-on: regression
```

Inputs: `version`, `config`, `fail-on`, `comment`, `github-token`, `working-directory`,
`allow-version-mismatch`, `binary-path` (use a local binary instead of a release; used by this repository's
own CI). Outputs: `verdict` (`pass`, `fail`, `error`), `regressions`, `report-path`.

The action downloads the release binary for the runner, verifies it against the release's `checksums.txt`
(failing on a mismatch), caches it by version, runs `soroban-gauge check`, writes the Markdown report to the
job summary, upserts **one** PR comment identified by `<!-- soroban-gauge:v1 -->`, uploads
`target/soroban-gauge/report.json` as an artifact, and exits with the CLI's exit code.

For pull requests from forks the token is read-only: no comment is posted (a notice says why) and the results
stay in the job summary. Only one comment per PR is kept, so with several configs set `comment: "false"` on all
but one.

## When a regression is intentional

```bash
soroban-gauge snapshot        # regenerate the baseline
git add .soroban-gauge/baseline.json
git commit -s -m "chore: update resource baseline"
```

After upgrading `soroban-sdk` (or this tool, when it embeds a newer host), `check` exits 4 until you re-run
`snapshot`: costs are not comparable across host versions.
