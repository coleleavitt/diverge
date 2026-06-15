//! Ported from research/portage/lib/portage/tests/versions/test_cpv_sort_key.py.

use diverge::version::sort_cpvs;

#[test]
fn cpv_sort_key_orders_like_portage() {
    let mut values: Vec<String> = ["a/b-2_alpha", "a", "b", "a/b-2", "a/a-1", "a/b-1"]
        .into_iter()
        .map(String::from)
        .collect();
    sort_cpvs(&mut values);
    assert_eq!(values, ["a", "a/a-1", "a/b-1", "a/b-2_alpha", "a/b-2", "b"],);
}
