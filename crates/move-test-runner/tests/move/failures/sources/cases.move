module failures::cases;

#[test]
fun passes() {}

#[test]
fun aborts() {
    abort 7
}

#[test, expected_failure(abort_code = 2, location = failures::cases)]
fun wrong_code() {
    abort 3
}

#[test, expected_failure]
fun does_not_abort() {}

#[test]
fun spins() {
    loop {}
}

// Tagged like a clever error, but this module has no constant for it to name.
#[test, expected_failure]
fun raw_tagged_code() {
    abort 0x8000_0000_0000_0000
}
