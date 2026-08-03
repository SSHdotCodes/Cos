# Contributing to Cos

Thanks for helping make Cos better. Small, focused changes are easiest to review.

## Before opening a pull request

1. Search existing issues and pull requests.
2. Keep unrelated changes in separate pull requests.
3. Add or update tests for behavior changes.
4. Run the checks below.
5. Explain user impact, implementation tradeoffs, and any security or migration concerns.

```sh
swift test --scratch-path /tmp/cos-test-build
npm test --prefix web
```

For native UI changes, also run `scripts/run_debug.sh` and include a current screenshot when appearance or interaction changes.

## Design principles

- Keep the native process lean and responsive.
- Keep one provider-neutral Cos harness instead of delegating to another agent harness.
- Make model capabilities explicit rather than showing controls a model cannot use.
- Keep secrets in Keychain and private state out of logs and task files.
- Keep plugin capabilities narrow, visible, and bounded.
- Preserve keyboard access, VoiceOver labels, reduced motion, and True Dark appearance.

## Security

Do not open a public issue for a vulnerability or include real credentials in a reproduction. Follow [SECURITY.md](SECURITY.md) instead.
