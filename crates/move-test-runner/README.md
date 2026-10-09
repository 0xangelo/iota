<!-- cargo-rdme start -->

Run a Move package's unit tests in-process, optionally collecting
instruction coverage.

Tests run the way `iota move test` runs them: same natives, unit-test gas
schedule and pass/fail rules. `Coverage::summary` prints what `iota move
coverage --dev summary` would.

Coverage comes from the in-memory hook behind `move-vm-runtime`'s `coverage`
feature, not from the CLI's per-instruction trace file. Other repositories
use the binary, published as a release by pushing a
`move-test-runner-v<version>` tag.

<!-- cargo-rdme end -->
