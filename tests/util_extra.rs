// Integration test for utility functions ported from upstream Portage tests.
// Source: research/portage/lib/portage/tests/util/

use diverge::util::stack_lists;

/// Test `stack_lists` function with examples from upstream test_stackDictList.py
/// This test ports observable behavior from:
/// research/portage/lib/portage/tests/util/test_stackDictList.py
///
/// The `stack_dictlist` function in Portage wraps `stack_lists` on the values.
/// We test that when dict values (which are lists) are stacked, they produce
/// the expected results with incremental mode enabled/disabled.
#[test]
fn test_stack_lists_from_stackdictlist_simple() {
    // From test_stackDictList.py line 13:
    // ({"a": "b"}, {"x": "y"}, False, {"a": ["b"], "x": ["y"]})
    // When converting to stack_lists, each dict becomes a list of one value.
    // With incremental=False, duplicates are prevented:
    // stack_lists([["a", "b"], ["x", "y"]], incremental=False) -> ["a", "b", "x", "y"]
    // But the actual test checks the dict structure, so we verify the stacking works correctly.

    // Simulating what stack_dictlist does: values are lists
    let dict_a_values = vec!["b".to_string()];
    let dict_x_values = vec!["y".to_string()];
    let combined = vec![dict_a_values, dict_x_values];
    let result = stack_lists(&combined, false);

    // With incremental=False, all items are kept in order without duplication
    assert_eq!(result, vec!["b", "y"]);
}

/// Test `stack_lists` with clearing operator `-*`
/// From test_stackDictList.py line 14:
/// ({"KEYWORDS": ["alpha", "x86"]}, {"KEYWORDS": ["-*"]}, True, {})
/// The `-*` operator clears all previous entries when incremental=True
#[test]
fn test_stack_lists_clear_operator() {
    let lists = vec![
        vec!["alpha".to_string(), "x86".to_string()],
        vec!["-*".to_string()],
    ];
    let result = stack_lists(&lists, true);

    // After the first list adds "alpha" and "x86", the second list with "-*"
    // clears everything, resulting in an empty list
    assert_eq!(result, Vec::<String>::new());
}

/// Test `stack_lists` with removal operator `-value`
/// From test_stackDictList.py line 15-20:
/// ({"KEYWORDS": ["alpha", "x86"]}, {"KEYWORDS": ["-x86"]}, True, {"KEYWORDS": ["alpha"]})
/// The `-x86` operator removes "x86" while keeping "alpha" when incremental=True
#[test]
fn test_stack_lists_removal_operator() {
    let lists = vec![
        vec!["alpha".to_string(), "x86".to_string()],
        vec!["-x86".to_string()],
    ];
    let result = stack_lists(&lists, true);

    // First list has ["alpha", "x86"], second removes "x86" with "-x86"
    // Result should be ["alpha"] with incremental=True
    assert_eq!(result, vec!["alpha"]);
}

/// Test `stack_lists` with incremental=True preserves order
/// Additional test to verify order preservation with multiple items
#[test]
fn test_stack_lists_incremental_order_preservation() {
    let lists = vec![
        vec!["a".to_string(), "b".to_string()],
        vec!["c".to_string()],
    ];
    let result = stack_lists(&lists, true);

    // With incremental=True, new items are added in order, no duplicates
    assert_eq!(result, vec!["a", "b", "c"]);
}

/// Test `stack_lists` with incremental=False (simple set union)
/// Items are kept in first-seen order without duplication
#[test]
fn test_stack_lists_non_incremental_union() {
    let lists = vec![
        vec!["a".to_string(), "b".to_string()],
        vec!["b".to_string(), "c".to_string()],
    ];
    let result = stack_lists(&lists, false);

    // With incremental=False, "b" appears only once (first-seen wins)
    assert_eq!(result, vec!["a", "b", "c"]);
}

/// Test `stack_lists` with multiple removal operators
#[test]
fn test_stack_lists_multiple_removals() {
    let lists = vec![
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        vec!["-a".to_string(), "-c".to_string()],
    ];
    let result = stack_lists(&lists, true);

    // Removes "a" and "c", keeps "b"
    assert_eq!(result, vec!["b"]);
}

/// Test `stack_lists` with removal before any addition in incremental mode
#[test]
fn test_stack_lists_leading_removal() {
    let lists = vec![vec!["-x".to_string(), "a".to_string()]];
    let result = stack_lists(&lists, true);

    // In incremental mode, "-x" tries to remove "x" from the list, but "x" isn't there,
    // so the removal is a no-op. Then "a" is added.
    assert_eq!(result, vec!["a"]);
}

/// Test `stack_lists` with removal before any addition in non-incremental mode
#[test]
fn test_stack_lists_leading_removal_non_incremental() {
    let lists = vec![vec!["-x".to_string(), "a".to_string()]];
    let result = stack_lists(&lists, false);

    // In non-incremental mode, "-x" is treated as a literal string (no special semantics)
    // and is added to the result.
    assert_eq!(result, vec!["-x", "a"]);
}

/// Test edge case: empty lists
#[test]
fn test_stack_lists_empty() {
    let lists: Vec<Vec<String>> = vec![];
    let result = stack_lists(&lists, true);
    assert_eq!(result, Vec::<String>::new());
}

/// Test edge case: empty inner lists
#[test]
fn test_stack_lists_empty_inner() {
    let lists = vec![vec![], vec!["a".to_string()], vec![]];
    let result = stack_lists(&lists, true);
    assert_eq!(result, vec!["a"]);
}
