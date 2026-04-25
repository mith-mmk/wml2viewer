use super::RenderLoadMetrics;

#[test]
fn render_load_metrics_default_is_zeroed() {
    let metrics = RenderLoadMetrics::default();

    assert_eq!(metrics.resolve_ms, 0);
    assert_eq!(metrics.read_ms, 0);
    assert_eq!(metrics.decode_ms, 0);
    assert_eq!(metrics.resize_ms, 0);
    assert!(!metrics.used_virtual_bytes);
    assert!(!metrics.decoded_from_bytes);
    assert!(metrics.source_bytes_len.is_none());
    assert!(metrics.resolved_path.is_none());
}
