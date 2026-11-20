//! 保证 include_dir!("$CARGO_MANIFEST_DIR/../../dist") 在编译期总有目录可读。
//!
//! - 正常构建（`npx tauri build` / `npm run build` 先跑）：仓库根 dist/ 已由
//!   vite 生成真实产物，本脚本不做任何改动。
//! - 全新 clone 仅 `cargo test/build`（未跑 npm build）：dist/ 不存在会让
//!   include_dir! 宏编译失败。此处建最小占位（index.html + 一个空 assets
//!   文件），保证编译通过、测试可跑；占位页不含真实 UI，仅用于开发期编译。
//!
//! dist/ 在 .gitignore 中，占位不会污染版本库。

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../../dist");
    println!("cargo:rerun-if-changed=build.rs");

    let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let dist = Path::new(&manifest).join("../..").join("dist");

    if dist.join("index.html").is_file() {
        return; // 真实产物已就位
    }

    let assets = dist.join("assets");
    if std::fs::create_dir_all(&assets).is_err() {
        return;
    }
    let _ = std::fs::write(
        dist.join("index.html"),
        b"<!DOCTYPE html><html lang=\"zh-CN\"><head><meta charset=\"UTF-8\"><title>OPX</title></head><body><div id=\"app\">OPX Web(dist placeholder; run npm run build)</div></body></html>",
    );
    let _ = std::fs::write(assets.join("placeholder.js"), b"/* dist placeholder */");
}
