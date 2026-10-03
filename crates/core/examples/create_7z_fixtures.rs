//! 生成 7z 测试 fixture 文件
//! 运行: cd src-tauri && cargo run --example create_7z_fixtures

use std::fs;
use std::io::Write;
use std::path::Path;

fn main() {
    let fixtures_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("fixtures")
        .join("7z");
    fs::create_dir_all(&fixtures_dir).expect("创建 fixtures/7z 目录");

    // 1. 创建源文件用于压缩
    let temp_dir = tempfile::tempdir().expect("创建临时目录");
    let src_dir = temp_dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("创建源目录");

    // 写入一些测试文件
    let mut f = fs::File::create(src_dir.join("hello.txt")).unwrap();
    f.write_all(b"Hello, World!\n").unwrap();

    let mut f = fs::File::create(src_dir.join("data.bin")).unwrap();
    f.write_all(&[0u8; 1024]).unwrap();

    let sub_dir = src_dir.join("subdir");
    fs::create_dir_all(&sub_dir).unwrap();
    let mut f = fs::File::create(sub_dir.join("nested.txt")).unwrap();
    f.write_all(b"Nested content\n").unwrap();

    // 2. 创建普通 7z (不加密)
    let normal_path = fixtures_dir.join("normal.7z");
    if normal_path.exists() {
        fs::remove_file(&normal_path).unwrap();
    }
    sevenz_rust::compress_to_path(&src_dir, &normal_path).expect("创建 normal.7z 失败");
    println!("✓ 已创建: {}", normal_path.display());

    // 3. 创建加密 7z (密码: test123)
    let encrypted_path = fixtures_dir.join("encrypted.7z");
    if encrypted_path.exists() {
        fs::remove_file(&encrypted_path).unwrap();
    }
    sevenz_rust::compress_to_path_encrypted(&src_dir, &encrypted_path, "test123".into())
        .expect("创建 encrypted.7z 失败");
    println!("✓ 已创建: {}", encrypted_path.display());

    // 4. 创建空 7z
    let empty_dir = temp_dir.path().join("empty_src");
    fs::create_dir_all(&empty_dir).expect("创建空源目录");
    let empty_path = fixtures_dir.join("empty.7z");
    if empty_path.exists() {
        fs::remove_file(&empty_path).unwrap();
    }
    sevenz_rust::compress_to_path(&empty_dir, &empty_path).expect("创建 empty.7z 失败");
    println!("✓ 已创建: {}", empty_path.display());

    println!("\n所有 7z fixture 文件已创建在: {}", fixtures_dir.display());
}
