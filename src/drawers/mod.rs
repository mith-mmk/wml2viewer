//! Small RGBA canvas and resampling helpers used by `wml2viewer`.
/*
  todo!
  1. 巨大な画像はリサイズしながら読み込む 上限512MB(default)　// オプションで切り替える(affine)で使用
  2. 回転画像の回転
簡単加工
  3. クロッピング
  4. フィルタ
  5. モノクロ化
  6. 保存

*/
pub mod affine;
pub mod canvas;
pub mod error;
pub mod image;
