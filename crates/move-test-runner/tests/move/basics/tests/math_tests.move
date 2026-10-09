#[test_only]
module basics::math_tests;

use basics::math;

#[test]
fun clamp_below_max() {
    assert!(math::clamp(1, 5) == 1);
}

#[test, expected_failure(abort_code = math::EDivisionByZero)]
fun divide_by_zero() {
    math::checked_div(1, 0);
}
