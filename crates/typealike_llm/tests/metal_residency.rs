//! Turns off the residency sets of Metal. The test binary has no other test, so no other thread
//! uses the environment while the test writes it.

#[test]
fn llama_cpp_gets_no_residency_sets() {
    // SAFETY: this binary runs no other test, so no other thread reads or writes the environment.
    unsafe { typealike_llm::disable_metal_residency_sets() };

    assert!(std::env::var_os("GGML_METAL_NO_RESIDENCY").is_some());
}
