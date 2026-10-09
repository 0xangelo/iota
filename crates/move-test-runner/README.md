<!-- cargo-rdme start -->

Run a Move package's unit tests in-process, optionally printing their
instruction coverage.

Tests run the way `iota move test` runs them: same natives, unit-test gas
schedule and pass/fail rules. With `--coverage`, it prints what `iota move
coverage --dev summary` would.

Coverage comes from the in-memory hook behind `move-vm-runtime`'s `coverage`
feature, not from the CLI's per-instruction trace file. Pushing a
`move-test-runner-v<version>` tag publishes the binary as a release.

<!-- cargo-rdme end -->
