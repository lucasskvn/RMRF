use rmrf_store::hash_data;

#[test]
fn hash_data_is_deterministic() {
    let data = b"test data for hashing";
    let h1 = hash_data(data);
    let h2 = hash_data(data);
    assert_eq!(h1, h2);
}

#[test]
fn hash_data_different_for_different_data() {
    let h1 = hash_data(b"data one");
    let h2 = hash_data(b"data two");
    assert_ne!(h1, h2);
}
