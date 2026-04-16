use std::cmp::Ordering;

pub(crate) fn compare_natural_str(left: &str, right: &str, case_sensitive: bool) -> Ordering {
    let left = if case_sensitive {
        left.to_string()
    } else {
        left.to_ascii_lowercase()
    };
    let right = if case_sensitive {
        right.to_string()
    } else {
        right.to_ascii_lowercase()
    };

    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();
    let mut li = 0;
    let mut ri = 0;

    while li < left_chars.len() && ri < right_chars.len() {
        let lc = left_chars[li];
        let rc = right_chars[ri];
        if lc.is_ascii_digit() && rc.is_ascii_digit() {
            let lstart = li;
            let rstart = ri;
            while li < left_chars.len() && left_chars[li].is_ascii_digit() {
                li += 1;
            }
            while ri < right_chars.len() && right_chars[ri].is_ascii_digit() {
                ri += 1;
            }
            let lnum = &left_chars[lstart..li];
            let rnum = &right_chars[rstart..ri];
            let ltrim = trim_leading_zeros(lnum);
            let rtrim = trim_leading_zeros(rnum);
            let len_cmp = ltrim.len().cmp(&rtrim.len());
            if len_cmp != Ordering::Equal {
                return len_cmp;
            }
            let digit_cmp = ltrim.iter().cmp(rtrim.iter());
            if digit_cmp != Ordering::Equal {
                return digit_cmp;
            }
            let raw_len_cmp = lnum.len().cmp(&rnum.len());
            if raw_len_cmp != Ordering::Equal {
                return raw_len_cmp;
            }
            continue;
        }

        let cmp = lc.cmp(&rc);
        if cmp != Ordering::Equal {
            return cmp;
        }
        li += 1;
        ri += 1;
    }

    left_chars.len().cmp(&right_chars.len())
}

pub(crate) fn compare_os_str(left: &str, right: &str) -> Ordering {
    #[cfg(target_os = "windows")]
    {
        return compare_windows_shell_str(left, right);
    }

    #[cfg(not(target_os = "windows"))]
    {
        compare_natural_str(
            &normalize_for_os_sort(left),
            &normalize_for_os_sort(right),
            true,
        )
    }
}

#[cfg(target_os = "windows")]
fn compare_windows_shell_str(left: &str, right: &str) -> Ordering {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "Shlwapi")]
    unsafe extern "system" {
        fn StrCmpLogicalW(left: *const u16, right: *const u16) -> i32;
    }

    let left_wide = std::ffi::OsStr::new(left)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let right_wide = std::ffi::OsStr::new(right)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe { StrCmpLogicalW(left_wide.as_ptr(), right_wide.as_ptr()) };
    result.cmp(&0)
}

#[cfg(not(target_os = "windows"))]
fn normalize_for_os_sort(input: &str) -> String {
    input.chars().flat_map(normalize_sort_char).collect()
}

#[cfg(not(target_os = "windows"))]
fn normalize_sort_char(ch: char) -> Vec<char> {
    let folded = match ch {
        '\u{30A1}'..='\u{30F6}' => char::from_u32(ch as u32 - 0x60).unwrap_or(ch),
        '\u{FF10}'..='\u{FF19}' => char::from_u32('0' as u32 + (ch as u32 - 0xFF10)).unwrap_or(ch),
        '\u{FF21}'..='\u{FF3A}' => char::from_u32('a' as u32 + (ch as u32 - 0xFF21)).unwrap_or(ch),
        '\u{FF41}'..='\u{FF5A}' => char::from_u32('a' as u32 + (ch as u32 - 0xFF41)).unwrap_or(ch),
        _ => ch,
    };
    folded.to_lowercase().collect()
}

fn trim_leading_zeros(chars: &[char]) -> &[char] {
    let trimmed = chars
        .iter()
        .position(|ch| *ch != '0')
        .unwrap_or(chars.len().saturating_sub(1));
    &chars[trimmed..]
}

#[cfg(test)]
mod tests {
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
}
