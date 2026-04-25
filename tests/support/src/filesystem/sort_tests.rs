use super::*;

#[test]
fn natural_sort_orders_numeric_suffixes() {
    assert_eq!(
        compare_natural_str("テスト10.jpg", "テスト2.jpg", false),
        Ordering::Greater
    );
}

#[test]
fn natural_sort_orders_parenthesized_numbers() {
    assert_eq!(
        compare_natural_str("テスト(5).jpg", "テスト(43).jpg", false),
        Ordering::Less
    );
}

#[test]
fn os_sort_treats_hiragana_and_katakana_similarly() {
    assert_eq!(compare_os_str("あ1", "ア2"), Ordering::Less);
}
