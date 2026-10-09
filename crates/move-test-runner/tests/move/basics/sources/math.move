module basics::math;

#[error]
const EDivisionByZero: vector<u8> = b"division by zero";

public fun clamp(x: u64, max: u64): u64 {
    if (x > max) max else x
}

public fun checked_div(a: u64, b: u64): u64 {
    assert!(b != 0, EDivisionByZero);
    a / b
}

public fun unused(): u64 {
    42
}
