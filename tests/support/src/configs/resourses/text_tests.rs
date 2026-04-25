use super::{UiTextKey, tr};

#[test]
fn newly_added_file_action_keys_are_localized_in_english() {
    assert_eq!(tr("en", UiTextKey::FileActionTarget), "Target");
    assert_eq!(
        tr("en", UiTextKey::UnsupportedFilesystemAction),
        "Unsupported filesystem action"
    );
    assert_eq!(tr("en", UiTextKey::UseAsMove), "Use as Move");
    assert_eq!(tr("en", UiTextKey::UseAsCopy), "Use as Copy");
    assert_eq!(tr("en", UiTextKey::AlertTitle), "Alert");
    assert_eq!(
        tr("en", UiTextKey::CurrentTargetNotEditableFile),
        "Current target is not editable file"
    );
}

#[test]
fn newly_added_file_action_keys_are_localized_in_japanese() {
    assert_eq!(tr("ja", UiTextKey::FileActionTarget), "対象");
    assert_eq!(
        tr("ja", UiTextKey::UnsupportedFilesystemAction),
        "未対応のファイル操作です"
    );
    assert_eq!(tr("ja", UiTextKey::UseAsMove), "移動先に使う");
    assert_eq!(tr("ja", UiTextKey::UseAsCopy), "コピー先に使う");
    assert_eq!(tr("ja", UiTextKey::AlertTitle), "アラート");
    assert_eq!(
        tr("ja", UiTextKey::CurrentTargetNotEditableFile),
        "現在の対象は編集可能なファイルではありません"
    );
}
