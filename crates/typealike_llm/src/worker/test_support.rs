//! The helpers that the tests of more than one module of the worker use.

pub(super) fn assert_near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-5, "{actual} is not {expected}");
}
